use std::{collections::HashMap, fmt, ops::Range};

use wgsl_parse::syntax::{
    AssignmentOperator, Attribute, BinaryOperator, CaseSelector, CompoundStatement, Declaration,
    DeclarationKind, Expression, ExpressionNode, FunctionCall, GlobalDeclaration,
    LiteralExpression, Statement, StatementNode, TranslationUnit, TypeExpression, UnaryOperator,
};
use wgsl_types::{
    Error as WgslTypeError,
    builtin::{type_builtin_fn, type_ctor, typecheck_struct_ctor},
    inst::LiteralInstance,
    syntax::BuiltinValue,
    tplt::TpltParam,
    ty::{StructMemberType, StructType, Type as WgslType},
};

use crate::builtin;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Ty {
    Unknown,
    Void,
    Bool,
    AbstractInt,
    AbstractFloat,
    I32,
    U32,
    F32,
    F16,
    Vector(u8, Box<Ty>),
    Matrix(u8, u8, Box<Ty>),
    Array(Box<Ty>, Option<u32>),
    Struct(String, Vec<(String, Ty)>),
    Pointer(Box<Ty>),
    Atomic(Box<Ty>),
    Sampler,
    Texture,
}

impl Ty {
    fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }

    fn contains_unknown(&self) -> bool {
        match self {
            Self::Unknown => true,
            Self::Vector(_, element)
            | Self::Matrix(_, _, element)
            | Self::Array(element, _)
            | Self::Pointer(element)
            | Self::Atomic(element) => element.contains_unknown(),
            Self::Struct(_, fields) => fields.iter().any(|(_, field)| field.contains_unknown()),
            _ => false,
        }
    }

    fn is_bool(&self) -> bool {
        matches!(self, Self::Bool)
    }

    fn is_numeric_scalar(&self) -> bool {
        matches!(
            self,
            Self::AbstractInt | Self::AbstractFloat | Self::I32 | Self::U32 | Self::F32 | Self::F16
        )
    }

    fn is_integer_scalar(&self) -> bool {
        matches!(self, Self::AbstractInt | Self::I32 | Self::U32)
    }

    fn is_numeric(&self) -> bool {
        self.is_numeric_scalar()
            || matches!(self, Self::Vector(_, element) | Self::Matrix(_, _, element) if element.is_numeric_scalar())
    }

    fn is_integer(&self) -> bool {
        self.is_integer_scalar()
            || matches!(self, Self::Vector(_, element) if element.is_integer_scalar())
    }

    fn element(&self) -> Option<&Ty> {
        match self {
            Self::Vector(_, element)
            | Self::Matrix(_, _, element)
            | Self::Array(element, _)
            | Self::Pointer(element)
            | Self::Atomic(element) => Some(element),
            _ => None,
        }
    }

    fn conversion_rank_to(&self, expected: &Ty) -> Option<u8> {
        if self.is_unknown() || expected.is_unknown() {
            return Some(0);
        }
        if self == expected {
            return Some(0);
        }
        if self.contains_unknown() || expected.contains_unknown() {
            return Some(0);
        }
        if matches!((self, expected), (Self::Pointer(_), Self::Pointer(_))) {
            return Some(0);
        }
        match (self, expected) {
            (Self::Array(left, left_size), Self::Array(right, right_size))
                if (right_size.is_none() || left_size == right_size)
                    && left.conversion_rank_to(right).is_some() =>
            {
                Some(2)
            }
            (Self::AbstractInt, Self::AbstractFloat) => Some(1),
            (Self::AbstractInt, Self::I32 | Self::U32) => Some(1),
            (Self::AbstractInt, Self::F32 | Self::F16) => Some(2),
            (Self::AbstractFloat, Self::F32 | Self::F16) => Some(1),
            (Self::I32 | Self::U32, Self::AbstractInt) => Some(1),
            (Self::F32 | Self::F16, Self::AbstractFloat) => Some(1),
            (Self::Vector(left_size, left), Self::Vector(right_size, right))
                if left_size == right_size =>
            {
                left.conversion_rank_to(right)
            }
            (Self::Matrix(left_c, left_r, left), Self::Matrix(right_c, right_r, right))
                if left_c == right_c && left_r == right_r =>
            {
                left.conversion_rank_to(right)
            }
            (Self::Array(left, left_size), Self::Array(right, right_size))
                if left_size == right_size =>
            {
                left.conversion_rank_to(right)
            }
            _ => None,
        }
    }

    fn concretize(self) -> Ty {
        match self {
            Self::AbstractInt => Self::I32,
            Self::AbstractFloat => Self::F32,
            Self::Vector(size, element) => Self::Vector(size, Box::new(element.concretize())),
            other => other,
        }
    }

    fn to_wgsl_type(&self) -> Option<WgslType> {
        match self {
            Self::Unknown | Self::Void | Self::Sampler | Self::Texture => None,
            Self::Bool => Some(WgslType::Bool),
            Self::AbstractInt => Some(WgslType::AbstractInt),
            Self::AbstractFloat => Some(WgslType::AbstractFloat),
            Self::I32 => Some(WgslType::I32),
            Self::U32 => Some(WgslType::U32),
            Self::F32 => Some(WgslType::F32),
            Self::F16 => Some(WgslType::F16),
            Self::Vector(size, element) => {
                Some(WgslType::Vec(*size, Box::new(element.to_wgsl_type()?)))
            }
            Self::Matrix(columns, rows, element) => Some(WgslType::Mat(
                *columns,
                *rows,
                Box::new(element.to_wgsl_type()?),
            )),
            Self::Array(element, size) => Some(WgslType::Array(
                Box::new(element.to_wgsl_type()?),
                size.map(|size| size as usize),
            )),
            Self::Struct(name, fields) => Some(WgslType::Struct(Box::new(StructType {
                name: name.clone(),
                members: fields
                    .iter()
                    .map(|(name, ty)| Some(StructMemberType::new(name.clone(), ty.to_wgsl_type()?)))
                    .collect::<Option<Vec<_>>>()?,
            }))),
            Self::Pointer(_) | Self::Atomic(_) => None,
        }
    }

    fn from_wgsl_type(ty: WgslType) -> Self {
        match ty {
            WgslType::Unknown => Self::Unknown,
            WgslType::Bool => Self::Bool,
            WgslType::AbstractInt => Self::AbstractInt,
            WgslType::AbstractFloat => Self::AbstractFloat,
            WgslType::I32 => Self::I32,
            WgslType::U32 => Self::U32,
            WgslType::F32 => Self::F32,
            WgslType::F16 => Self::F16,
            WgslType::Vec(size, element) => {
                Self::Vector(size, Box::new(Self::from_wgsl_type(*element)))
            }
            WgslType::Mat(columns, rows, element) => {
                Self::Matrix(columns, rows, Box::new(Self::from_wgsl_type(*element)))
            }
            WgslType::Array(element, size) => Self::Array(
                Box::new(Self::from_wgsl_type(*element)),
                size.and_then(|size| u32::try_from(size).ok()),
            ),
            WgslType::Struct(structure) => Self::Struct(
                structure.name,
                structure
                    .members
                    .into_iter()
                    .map(|member| (member.name, Self::from_wgsl_type(member.ty)))
                    .collect(),
            ),
            WgslType::Ptr(_, element, _) | WgslType::Ref(_, element, _) => {
                Self::Pointer(Box::new(Self::from_wgsl_type(*element)))
            }
            WgslType::Atomic(element) => Self::Atomic(Box::new(Self::from_wgsl_type(*element))),
            WgslType::Sampler(_) => Self::Sampler,
            WgslType::Texture(_) => Self::Texture,
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Unknown => write!(formatter, "unknown"),
            Ty::Void => write!(formatter, "void"),
            Ty::Bool => write!(formatter, "bool"),
            Ty::AbstractInt => write!(formatter, "abstract-int"),
            Ty::AbstractFloat => write!(formatter, "abstract-float"),
            Ty::I32 => write!(formatter, "i32"),
            Ty::U32 => write!(formatter, "u32"),
            Ty::F32 => write!(formatter, "f32"),
            Ty::F16 => write!(formatter, "f16"),
            Ty::Vector(size, element) => write!(formatter, "vec{size}<{element}>"),
            Ty::Matrix(columns, rows, element) => {
                write!(formatter, "mat{columns}x{rows}<{element}>")
            }
            Ty::Array(element, Some(size)) => write!(formatter, "array<{element}, {size}>"),
            Ty::Array(element, None) => write!(formatter, "array<{element}>"),
            Ty::Struct(name, _) => write!(formatter, "{name}"),
            Ty::Pointer(element) => write!(formatter, "ptr<{element}>"),
            Ty::Atomic(element) => write!(formatter, "atomic<{element}>"),
            Ty::Sampler => write!(formatter, "sampler"),
            Ty::Texture => write!(formatter, "texture"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeDiagnostic {
    pub range: Range<usize>,
    pub message: String,
    pub related: Vec<(Range<usize>, String)>,
}

#[derive(Clone, Debug)]
struct FunctionTy {
    parameters: Vec<Ty>,
    result: Ty,
    declaration: Range<usize>,
}

#[derive(Clone, Default)]
pub(crate) struct TypeEnvironment {
    named: HashMap<String, Ty>,
    globals: HashMap<String, Ty>,
    functions: HashMap<String, FunctionTy>,
}

impl TypeEnvironment {
    pub(crate) fn bind_alias(&mut self, local_name: &str, original_name: &str, source: &Self) {
        if let Some(ty) = source.named.get(original_name) {
            self.named.insert(local_name.to_owned(), ty.clone());
        }
        if let Some(ty) = source.globals.get(original_name) {
            self.globals.insert(local_name.to_owned(), ty.clone());
        }
        if let Some(function) = source.functions.get(original_name) {
            self.functions
                .insert(local_name.to_owned(), function.clone());
        }
    }
}

pub(crate) fn analyze_module(
    module: &TranslationUnit,
    types: TypeEnvironment,
) -> (Vec<TypeDiagnostic>, TypeEnvironment) {
    let mut checker = Checker {
        module,
        types,
        diagnostics: Vec::new(),
    };
    checker.collect_types();
    checker.check_globals();
    checker.check_functions();
    checker.check_const_asserts();
    (checker.diagnostics, checker.types)
}

pub fn check_module(module: &TranslationUnit) -> Vec<TypeDiagnostic> {
    analyze_module(module, TypeEnvironment::default()).0
}

pub(crate) fn infer_expression_type(
    module: &TranslationUnit,
    expression: Expression,
    local_type_names: &HashMap<String, String>,
    types: TypeEnvironment,
) -> Ty {
    let mut checker = Checker {
        module,
        types,
        diagnostics: Vec::new(),
    };
    checker.collect_types();
    checker.check_globals();
    let locals = local_type_names
        .iter()
        .filter_map(|(name, type_name)| {
            let source = if (type_name.starts_with("vec") || type_name.starts_with("mat"))
                && !type_name.contains('<')
            {
                format!("alias __LocalType = {type_name}<f32>;")
            } else {
                format!("alias __LocalType = {type_name};")
            };
            let wrapper = wgsl_parse::parse_str(&source).ok()?;
            let ty = wrapper
                .global_declarations
                .into_iter()
                .find_map(|declaration| {
                    if let GlobalDeclaration::TypeAlias(alias) = declaration.into_inner() {
                        Some(alias.ty)
                    } else {
                        None
                    }
                })?;
            Some((name.clone(), checker.resolve_type(&ty)))
        })
        .collect();
    checker.infer_expression(&ExpressionNode::from(expression), &locals)
}
struct Checker<'a> {
    module: &'a TranslationUnit,
    types: TypeEnvironment,
    diagnostics: Vec<TypeDiagnostic>,
}

impl Checker<'_> {
    fn collect_types(&mut self) {
        for declaration in &self.module.global_declarations {
            if let GlobalDeclaration::Struct(structure) = declaration.node() {
                self.types.named.insert(
                    structure.ident.name().to_string(),
                    Ty::Struct(structure.ident.name().to_string(), Vec::new()),
                );
            }
        }
        for declaration in &self.module.global_declarations {
            match declaration.node() {
                GlobalDeclaration::Struct(structure) => {
                    let mut fields = Vec::with_capacity(structure.members.len());
                    for member in &structure.members {
                        let ty = self.resolve_type(&member.ty);
                        self.check_io_attributes(
                            &member.attributes,
                            &ty,
                            declaration.span().range(),
                        );
                        fields.push((member.ident.name().to_string(), ty));
                    }
                    self.types.named.insert(
                        structure.ident.name().to_string(),
                        Ty::Struct(structure.ident.name().to_string(), fields),
                    );
                }
                GlobalDeclaration::TypeAlias(alias) => {
                    let ty = self.resolve_type(&alias.ty);
                    self.types.named.insert(alias.ident.name().to_string(), ty);
                }
                _ => {}
            }
        }
        for declaration in &self.module.global_declarations {
            match declaration.node() {
                GlobalDeclaration::Function(function) => {
                    let function_ty = FunctionTy {
                        parameters: function
                            .parameters
                            .iter()
                            .map(|parameter| self.resolve_type(&parameter.ty))
                            .collect(),
                        result: function
                            .return_type
                            .as_ref()
                            .map(|ty| self.resolve_type(ty))
                            .unwrap_or(Ty::Void),
                        declaration: declaration.span().range(),
                    };
                    self.types
                        .functions
                        .insert(function.ident.name().to_string(), function_ty);
                }
                GlobalDeclaration::Declaration(global) => {
                    let ty = global
                        .ty
                        .as_ref()
                        .map(|ty| self.resolve_type(ty))
                        .unwrap_or(Ty::Unknown);
                    self.types
                        .globals
                        .insert(global.ident.name().to_string(), ty);
                }
                _ => {}
            }
        }
    }

    fn check_globals(&mut self) {
        let mut inferred = Vec::new();
        for declaration in &self.module.global_declarations {
            let GlobalDeclaration::Declaration(global) = declaration.node() else {
                continue;
            };
            let mut locals = self.types.globals.clone();
            let actual = global
                .initializer
                .as_ref()
                .map(|initializer| self.infer_expression(initializer, &locals))
                .unwrap_or(Ty::Unknown);
            let expected = global
                .ty
                .as_ref()
                .map(|ty| self.resolve_type(ty))
                .unwrap_or_else(|| {
                    if global.kind == DeclarationKind::Const {
                        actual.clone()
                    } else {
                        actual.clone().concretize()
                    }
                });
            if let Some(initializer) = &global.initializer {
                self.check_compatible(
                    initializer.span().range(),
                    &actual,
                    &expected,
                    Some((declaration.span().range(), "declared here".to_owned())),
                );
            }
            locals.insert(global.ident.name().to_string(), expected.clone());
            inferred.push((global.ident.name().to_string(), expected));
        }
        self.types.globals.extend(inferred);
    }

    fn check_functions(&mut self) {
        for declaration in &self.module.global_declarations {
            let GlobalDeclaration::Function(function) = declaration.node() else {
                continue;
            };
            let signature = self
                .types
                .functions
                .get(function.ident.name().as_str())
                .cloned()
                .unwrap();
            self.check_function_attributes(function, declaration.span().range());
            let mut locals = self.types.globals.clone();
            for (parameter, ty) in function.parameters.iter().zip(&signature.parameters) {
                locals.insert(parameter.ident.name().to_string(), ty.clone());
                self.check_io_attributes(&parameter.attributes, ty, declaration.span().range());
            }
            self.check_io_attributes(
                &function.return_attributes,
                &signature.result,
                declaration.span().range(),
            );
            self.check_compound(
                &function.body,
                &mut locals,
                &signature.result,
                &signature.declaration,
            );
        }
    }

    fn check_const_asserts(&mut self) {
        for declaration in &self.module.global_declarations {
            let GlobalDeclaration::ConstAssert(assertion) = declaration.node() else {
                continue;
            };
            let (result, _) = wesl::eval(assertion.expression.node(), self.module);
            if matches!(
                result,
                Ok(wesl::eval::Instance::Literal(
                    wesl::eval::LiteralInstance::Bool(false)
                ))
            ) {
                self.diagnostics.push(TypeDiagnostic {
                    range: assertion.expression.span().range(),
                    message: "const assertion evaluates to false".to_owned(),
                    related: Vec::new(),
                });
            }
        }
    }

    fn check_function_attributes(
        &mut self,
        function: &wgsl_parse::syntax::Function,
        declaration: Range<usize>,
    ) {
        let globals = self.types.globals.clone();
        for attribute in &function.attributes {
            let Attribute::WorkgroupSize(size) = attribute.node() else {
                continue;
            };
            let expressions = std::iter::once(&size.x)
                .chain(size.y.iter())
                .chain(size.z.iter());
            for expression in expressions {
                let ty = self.infer_expression(expression, &globals);
                if !ty.is_unknown() && !ty.is_integer_scalar() {
                    self.diagnostics.push(TypeDiagnostic {
                        range: expression.span().range(),
                        message: format!("@workgroup_size value must be an integer, found {ty}"),
                        related: vec![(
                            declaration.clone(),
                            "entry point declared here".to_owned(),
                        )],
                    });
                } else if const_u32(self.module, expression) == Some(0) {
                    self.diagnostics.push(TypeDiagnostic {
                        range: expression.span().range(),
                        message: "@workgroup_size value must be greater than zero".to_owned(),
                        related: vec![(
                            declaration.clone(),
                            "entry point declared here".to_owned(),
                        )],
                    });
                }
            }
        }
    }

    fn check_io_attributes(
        &mut self,
        attributes: &[wgsl_parse::syntax::AttributeNode],
        ty: &Ty,
        declaration: Range<usize>,
    ) {
        for attribute in attributes {
            match attribute.node() {
                Attribute::Location(_)
                    if !ty.is_unknown()
                        && !ty.is_numeric_scalar()
                        && !matches!(ty, Ty::Vector(_, element) if element.is_numeric_scalar()) =>
                {
                    self.diagnostics.push(TypeDiagnostic {
                        range: attribute.span().range(),
                        message: format!(
                            "@location value must be a numeric scalar or vector, found {ty}"
                        ),
                        related: vec![(
                            declaration.clone(),
                            "entry point declared here".to_owned(),
                        )],
                    });
                }
                Attribute::Builtin(value) => {
                    if let Some(expected) = builtin_value_type(*value)
                        && !ty.is_unknown()
                        && ty.conversion_rank_to(&expected).is_none()
                    {
                        self.diagnostics.push(TypeDiagnostic {
                            range: attribute.span().range(),
                            message: format!("@builtin value requires type {expected}, found {ty}"),
                            related: vec![(
                                declaration.clone(),
                                "entry point declared here".to_owned(),
                            )],
                        });
                    }
                }
                _ => {}
            }
        }
    }

    fn check_compound(
        &mut self,
        compound: &CompoundStatement,
        locals: &mut HashMap<String, Ty>,
        return_ty: &Ty,
        function_declaration: &Range<usize>,
    ) {
        let mut scoped = locals.clone();
        for statement in &compound.statements {
            self.check_statement(statement, &mut scoped, return_ty, function_declaration);
        }
    }

    fn check_statement(
        &mut self,
        statement: &StatementNode,
        locals: &mut HashMap<String, Ty>,
        return_ty: &Ty,
        function_declaration: &Range<usize>,
    ) {
        match statement.node() {
            Statement::Void
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Discard(_) => {}
            Statement::Compound(compound) => {
                self.check_compound(compound, locals, return_ty, function_declaration)
            }
            Statement::Declaration(declaration) => {
                self.check_declaration(declaration, statement.span().range(), locals)
            }
            Statement::Assignment(assignment) => {
                let left = self.infer_expression(&assignment.lhs, locals);
                let right = self.infer_expression(&assignment.rhs, locals);
                if assignment.operator == AssignmentOperator::Equal {
                    self.check_compatible(
                        assignment.rhs.span().range(),
                        &right,
                        &left,
                        Some((
                            assignment.lhs.span().range(),
                            "assignment target".to_owned(),
                        )),
                    );
                } else if let Some(operator) = assignment_binary_operator(assignment.operator) {
                    let result = infer_binary_result(operator, left.clone(), right.clone());
                    if result.is_unknown() && !left.contains_unknown() && !right.contains_unknown()
                    {
                        self.invalid_operator(
                            statement.span().range(),
                            format!(
                                "operator {:?} is not defined for {left} and {right}",
                                assignment.operator
                            ),
                        );
                    } else {
                        self.check_compatible(
                            assignment.rhs.span().range(),
                            &result,
                            &left,
                            Some((
                                assignment.lhs.span().range(),
                                "assignment target".to_owned(),
                            )),
                        );
                    }
                }
            }
            Statement::Increment(increment) => {
                let ty = self.infer_expression(&increment.expression, locals);
                self.check_numeric(increment.expression.span().range(), &ty);
            }
            Statement::Decrement(decrement) => {
                let ty = self.infer_expression(&decrement.expression, locals);
                self.check_numeric(decrement.expression.span().range(), &ty);
            }
            Statement::If(if_statement) => {
                let condition = self.infer_expression(&if_statement.if_clause.expression, locals);
                self.check_bool(if_statement.if_clause.expression.span().range(), &condition);
                self.check_compound(
                    &if_statement.if_clause.body,
                    locals,
                    return_ty,
                    function_declaration,
                );
                for clause in &if_statement.else_if_clauses {
                    let condition = self.infer_expression(&clause.expression, locals);
                    self.check_bool(clause.expression.span().range(), &condition);
                    self.check_compound(&clause.body, locals, return_ty, function_declaration);
                }
                if let Some(clause) = &if_statement.else_clause {
                    self.check_compound(&clause.body, locals, return_ty, function_declaration);
                }
            }
            Statement::Switch(switch) => {
                let selector = self.infer_expression(&switch.expression, locals);
                if !selector.is_unknown() && !selector.is_integer_scalar() {
                    self.diagnostics.push(TypeDiagnostic {
                        range: switch.expression.span().range(),
                        message: format!("switch selector must be an integer, found {selector}"),
                        related: Vec::new(),
                    });
                }
                for clause in &switch.clauses {
                    for case in &clause.case_selectors {
                        if let CaseSelector::Expression(expression) = case {
                            let case_ty = self.infer_expression(expression, locals);
                            self.check_compatible(
                                expression.span().range(),
                                &case_ty,
                                &selector,
                                None,
                            );
                        }
                    }
                    self.check_compound(&clause.body, locals, return_ty, function_declaration);
                }
            }
            Statement::Loop(loop_statement) => {
                self.check_compound(
                    &loop_statement.body,
                    locals,
                    return_ty,
                    function_declaration,
                );
                if let Some(continuing) = &loop_statement.continuing {
                    self.check_compound(&continuing.body, locals, return_ty, function_declaration);
                    if let Some(break_if) = &continuing.break_if {
                        let ty = self.infer_expression(&break_if.expression, locals);
                        self.check_bool(break_if.expression.span().range(), &ty);
                    }
                }
            }
            Statement::For(for_statement) => {
                let mut scoped = locals.clone();
                if let Some(initializer) = &for_statement.initializer {
                    self.check_statement(initializer, &mut scoped, return_ty, function_declaration);
                }
                if let Some(condition) = &for_statement.condition {
                    let ty = self.infer_expression(condition, &scoped);
                    self.check_bool(condition.span().range(), &ty);
                }
                if let Some(update) = &for_statement.update {
                    self.check_statement(update, &mut scoped, return_ty, function_declaration);
                }
                self.check_compound(
                    &for_statement.body,
                    &mut scoped,
                    return_ty,
                    function_declaration,
                );
            }
            Statement::While(while_statement) => {
                let ty = self.infer_expression(&while_statement.condition, locals);
                self.check_bool(while_statement.condition.span().range(), &ty);
                self.check_compound(
                    &while_statement.body,
                    locals,
                    return_ty,
                    function_declaration,
                );
            }
            Statement::Return(return_statement) => {
                let actual = return_statement
                    .expression
                    .as_ref()
                    .map(|expression| self.infer_expression(expression, locals))
                    .unwrap_or(Ty::Void);
                let range = return_statement
                    .expression
                    .as_ref()
                    .map(|expression| expression.span().range())
                    .unwrap_or_else(|| statement.span().range());
                self.check_compatible(
                    range,
                    &actual,
                    return_ty,
                    Some((
                        function_declaration.clone(),
                        "function return type".to_owned(),
                    )),
                );
            }
            Statement::FunctionCall(call) => {
                self.infer_call(&call.call, statement.span().range(), locals);
            }
            Statement::ConstAssert(assertion) => {
                let ty = self.infer_expression(&assertion.expression, locals);
                self.check_bool(assertion.expression.span().range(), &ty);
            }
        }
    }

    fn check_declaration(
        &mut self,
        declaration: &Declaration,
        declaration_range: Range<usize>,
        locals: &mut HashMap<String, Ty>,
    ) {
        let actual = declaration
            .initializer
            .as_ref()
            .map(|initializer| self.infer_expression(initializer, locals))
            .unwrap_or(Ty::Unknown);
        let expected = declaration
            .ty
            .as_ref()
            .map(|ty| self.resolve_type(ty))
            .unwrap_or_else(|| {
                if declaration.kind == DeclarationKind::Const {
                    actual.clone()
                } else {
                    actual.clone().concretize()
                }
            });
        if let Some(initializer) = &declaration.initializer {
            self.check_compatible(
                initializer.span().range(),
                &actual,
                &expected,
                Some((declaration_range, "declared here".to_owned())),
            );
        }
        locals.insert(declaration.ident.name().to_string(), expected);
    }

    fn infer_expression(
        &mut self,
        expression: &ExpressionNode,
        locals: &HashMap<String, Ty>,
    ) -> Ty {
        match expression.node() {
            Expression::Literal(literal) => match literal {
                LiteralExpression::Bool(_) => Ty::Bool,
                LiteralExpression::AbstractInt(_) => Ty::AbstractInt,
                LiteralExpression::AbstractFloat(_) => Ty::AbstractFloat,
                LiteralExpression::I32(_) => Ty::I32,
                LiteralExpression::U32(_) => Ty::U32,
                LiteralExpression::F32(_) => Ty::F32,
                LiteralExpression::F16(_) => Ty::F16,
            },
            Expression::Parenthesized(parenthesized) => {
                self.infer_expression(&parenthesized.expression, locals)
            }
            Expression::TypeOrIdentifier(ty) => {
                if ty.path.is_some() {
                    Ty::Unknown
                } else {
                    locals
                        .get(ty.ident.name().as_str())
                        .or_else(|| self.types.globals.get(ty.ident.name().as_str()))
                        .cloned()
                        .unwrap_or(Ty::Unknown)
                }
            }
            Expression::NamedComponent(component) => {
                let base = self.infer_expression(&component.base, locals);
                let name = component.component.name();
                member_type(base, &name).unwrap_or_else(|| {
                    let expression_range = expression.span().range();
                    let member_end = expression_range.end;
                    let member_start = member_end
                        .saturating_sub(name.len())
                        .max(expression_range.start);
                    self.diagnostics.push(TypeDiagnostic {
                        range: member_start..member_end,
                        message: format!("type has no member {name}"),
                        related: Vec::new(),
                    });
                    Ty::Unknown
                })
            }
            Expression::Indexing(indexing) => {
                let base = self.infer_expression(&indexing.base, locals);
                let index = self.infer_expression(&indexing.index, locals);
                if !index.is_unknown() && !index.is_integer_scalar() {
                    self.diagnostics.push(TypeDiagnostic {
                        range: indexing.index.span().range(),
                        message: format!("index must be an integer, found {index}"),
                        related: Vec::new(),
                    });
                }
                index_result(base)
            }
            Expression::Unary(unary) => {
                let operand = self.infer_expression(&unary.operand, locals);
                match unary.operator {
                    UnaryOperator::LogicalNegation => match operand {
                        Ty::Bool => Ty::Bool,
                        Ty::Vector(size, element) if element.is_bool() || element.is_unknown() => {
                            Ty::Vector(size, element)
                        }
                        Ty::Unknown => Ty::Unknown,
                        other => {
                            self.invalid_operator(
                                expression.span().range(),
                                format!("operator ! is not defined for {other}"),
                            );
                            Ty::Unknown
                        }
                    },
                    UnaryOperator::AddressOf => Ty::Pointer(Box::new(operand)),
                    UnaryOperator::Indirection => match operand {
                        Ty::Pointer(inner) => *inner,
                        Ty::Unknown => Ty::Unknown,
                        other => {
                            self.invalid_operator(
                                expression.span().range(),
                                format!("cannot dereference {other}"),
                            );
                            Ty::Unknown
                        }
                    },
                    UnaryOperator::Negation if operand.is_numeric() => operand,
                    UnaryOperator::BitwiseComplement if operand.is_integer() => operand,
                    _ if operand.is_unknown() => Ty::Unknown,
                    _ => {
                        self.invalid_operator(
                            expression.span().range(),
                            format!("operator {:?} is not defined for {operand}", unary.operator),
                        );
                        Ty::Unknown
                    }
                }
            }
            Expression::Binary(binary) => {
                let left = self.infer_expression(&binary.left, locals);
                let right = self.infer_expression(&binary.right, locals);
                if left.contains_unknown() || right.contains_unknown() {
                    return infer_binary_result(binary.operator, left, right);
                }
                let result = infer_binary_result(binary.operator, left.clone(), right.clone());
                if result.is_unknown() {
                    self.invalid_operator(
                        expression.span().range(),
                        format!(
                            "operator {:?} is not defined for {left} and {right}",
                            binary.operator
                        ),
                    );
                }
                result
            }
            Expression::FunctionCall(call) => {
                self.infer_call(call, expression.span().range(), locals)
            }
        }
    }

    fn infer_call(
        &mut self,
        call: &FunctionCall,
        range: Range<usize>,
        locals: &HashMap<String, Ty>,
    ) -> Ty {
        let arguments: Vec<_> = call
            .arguments
            .iter()
            .map(|argument| self.infer_expression(argument, locals))
            .collect();
        if call.ty.path.is_some() {
            return Ty::Unknown;
        }
        let name = call.ty.ident.name().to_string();
        if let Some(function) = self.types.functions.get(&name).cloned() {
            for (index, (actual, expected)) in
                arguments.iter().zip(&function.parameters).enumerate()
            {
                if let Some(argument) = call.arguments.get(index) {
                    self.check_compatible(
                        argument.span().range(),
                        actual,
                        expected,
                        Some((
                            function.declaration.clone(),
                            "function declared here".to_owned(),
                        )),
                    );
                }
            }
            if arguments.len() != function.parameters.len() {
                self.diagnostics.push(TypeDiagnostic {
                    range,
                    message: format!(
                        "function {name} expects {} arguments, found {}",
                        function.parameters.len(),
                        arguments.len()
                    ),
                    related: vec![(function.declaration, "function declared here".to_owned())],
                });
            }
            return function.result;
        }
        let constructor = self.resolve_type(&call.ty);
        if !constructor.is_unknown() {
            if arguments.iter().any(Ty::contains_unknown) {
                return fill_constructor_type(constructor, &arguments);
            }
            if !constructor_shape_valid(&constructor, &arguments) {
                self.diagnostics.push(TypeDiagnostic {
                    range,
                    message: format!("invalid constructor {name}: argument shape does not match"),
                    related: Vec::new(),
                });
                return Ty::Unknown;
            }
            let Some(argument_types) = arguments
                .iter()
                .map(Ty::to_wgsl_type)
                .collect::<Option<Vec<_>>>()
            else {
                return constructor;
            };
            let result = if matches!(constructor, Ty::Struct(_, _)) {
                let Some(WgslType::Struct(structure)) = constructor.to_wgsl_type() else {
                    return constructor;
                };
                typecheck_struct_ctor(&structure, &argument_types)
                    .map(|()| WgslType::Struct(structure))
            } else if let Some(constructor_name) = constructor_name(&constructor) {
                let templates = constructor_templates(&constructor);
                type_ctor(&constructor_name, templates.as_deref(), &argument_types)
            } else {
                self.diagnostics.push(TypeDiagnostic {
                    range,
                    message: format!("{name} is not constructible"),
                    related: Vec::new(),
                });
                return Ty::Unknown;
            };
            return match result {
                Ok(result) => Ty::from_wgsl_type(result),
                Err(error) => {
                    self.diagnostics.push(TypeDiagnostic {
                        range,
                        message: format!("invalid constructor {name}: {error}"),
                        related: Vec::new(),
                    });
                    Ty::Unknown
                }
            };
        }
        if let Some(function) = builtin(&name) {
            if arguments.iter().any(Ty::contains_unknown) {
                return Ty::Unknown;
            }
            let Ok(templates) = self.template_parameters(&call.ty) else {
                return Ty::Unknown;
            };
            match resolve_builtin_overload(&name, templates.as_deref(), &arguments) {
                BuiltinResolution::Match(Some(result)) => return Ty::from_wgsl_type(result),
                BuiltinResolution::Match(None) => return Ty::Void,
                BuiltinResolution::Unknown => return Ty::Unknown,
                BuiltinResolution::NoMatch(error) => {
                    let candidates = function
                        .overloads
                        .iter()
                        .map(|overload| overload.signature)
                        .collect::<Vec<_>>()
                        .join("; ");
                    self.diagnostics.push(TypeDiagnostic {
                        range,
                        message: format!(
                            "no matching overload for {name}({}): {error}; candidates: {candidates}",
                            arguments
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        related: Vec::new(),
                    });
                    return Ty::Unknown;
                }
            }
        }
        Ty::Unknown
    }

    fn template_parameters(&self, ty: &TypeExpression) -> Result<Option<Vec<TpltParam>>, ()> {
        ty.template_args
            .as_ref()
            .map(|arguments| {
                arguments
                    .iter()
                    .map(|argument| match argument.expression.node() {
                        Expression::TypeOrIdentifier(ty) => self
                            .resolve_type(ty)
                            .to_wgsl_type()
                            .map(TpltParam::Type)
                            .ok_or(()),
                        Expression::Literal(LiteralExpression::AbstractInt(value)) => Ok(
                            TpltParam::Instance(LiteralInstance::AbstractInt(*value).into()),
                        ),
                        Expression::Literal(LiteralExpression::U32(value)) => {
                            Ok(TpltParam::Instance(LiteralInstance::U32(*value).into()))
                        }
                        _ => Err(()),
                    })
                    .collect()
            })
            .transpose()
    }

    fn resolve_type(&self, ty: &TypeExpression) -> Ty {
        if ty.path.is_some() {
            return Ty::Unknown;
        }
        let name = ty.ident.name();
        match name.as_str() {
            "bool" => Ty::Bool,
            "i32" => Ty::I32,
            "u32" => Ty::U32,
            "f32" => Ty::F32,
            "f16" => Ty::F16,
            "vec2" | "vec3" | "vec4" => {
                let size = name.as_bytes()[3] - b'0';
                Ty::Vector(size, Box::new(self.template_type(ty, 0)))
            }
            name if name.len() == 5 && name.starts_with("vec") => {
                let bytes = name.as_bytes();
                Ty::Vector(
                    bytes[3] - b'0',
                    Box::new(shorthand_scalar(bytes[4]).unwrap_or(Ty::Unknown)),
                )
            }
            name if name.starts_with("mat") && name.len() == 6 => {
                let bytes = name.as_bytes();
                Ty::Matrix(
                    bytes[3] - b'0',
                    bytes[5] - b'0',
                    Box::new(self.template_type(ty, 0)),
                )
            }
            name if name.len() == 7 && name.starts_with("mat") => {
                let bytes = name.as_bytes();
                Ty::Matrix(
                    bytes[3] - b'0',
                    bytes[5] - b'0',
                    Box::new(shorthand_scalar(bytes[6]).unwrap_or(Ty::Unknown)),
                )
            }
            "array" => Ty::Array(
                Box::new(self.template_type(ty, 0)),
                ty.template_args
                    .as_ref()
                    .and_then(|arguments| arguments.get(1))
                    .and_then(|argument| const_u32(self.module, &argument.expression)),
            ),
            "ptr" => Ty::Pointer(Box::new(self.template_type(ty, 1))),
            "atomic" => Ty::Atomic(Box::new(self.template_type(ty, 0))),
            "sampler" | "sampler_comparison" => Ty::Sampler,
            name if name.starts_with("texture_") => Ty::Texture,
            name => self.types.named.get(name).cloned().unwrap_or(Ty::Unknown),
        }
    }

    fn template_type(&self, ty: &TypeExpression, index: usize) -> Ty {
        ty.template_args
            .as_ref()
            .and_then(|arguments| arguments.get(index))
            .and_then(|argument| match argument.expression.node() {
                Expression::TypeOrIdentifier(ty) => Some(self.resolve_type(ty)),
                _ => None,
            })
            .unwrap_or(Ty::Unknown)
    }

    fn check_compatible(
        &mut self,
        range: Range<usize>,
        actual: &Ty,
        expected: &Ty,
        related: Option<(Range<usize>, String)>,
    ) {
        if actual.conversion_rank_to(expected).is_none() {
            let actual = actual.clone().concretize();
            self.diagnostics.push(TypeDiagnostic {
                range,
                message: format!("type mismatch: expected {expected}, found {actual}"),
                related: related.into_iter().collect(),
            });
        }
    }

    fn check_bool(&mut self, range: Range<usize>, ty: &Ty) {
        if !ty.is_unknown() && !ty.is_bool() {
            self.diagnostics.push(TypeDiagnostic {
                range,
                message: format!("condition must be bool, found {ty}"),
                related: Vec::new(),
            });
        }
    }

    fn check_numeric(&mut self, range: Range<usize>, ty: &Ty) {
        if !ty.is_unknown() && !ty.is_numeric_scalar() {
            self.diagnostics.push(TypeDiagnostic {
                range,
                message: format!("expected numeric scalar, found {ty}"),
                related: Vec::new(),
            });
        }
    }
    fn invalid_operator(&mut self, range: Range<usize>, message: String) {
        self.diagnostics.push(TypeDiagnostic {
            range,
            message,
            related: Vec::new(),
        });
    }
}

fn builtin_value_type(value: BuiltinValue) -> Option<Ty> {
    match value {
        BuiltinValue::VertexIndex
        | BuiltinValue::InstanceIndex
        | BuiltinValue::SampleIndex
        | BuiltinValue::SampleMask
        | BuiltinValue::LocalInvocationIndex => Some(Ty::U32),
        BuiltinValue::Position => Some(Ty::Vector(4, Box::new(Ty::F32))),
        BuiltinValue::FrontFacing => Some(Ty::Bool),
        BuiltinValue::FragDepth => Some(Ty::F32),
        BuiltinValue::LocalInvocationId
        | BuiltinValue::GlobalInvocationId
        | BuiltinValue::WorkgroupId
        | BuiltinValue::NumWorkgroups => Some(Ty::Vector(3, Box::new(Ty::U32))),
        BuiltinValue::ClipDistances
        | BuiltinValue::SubgroupInvocationId
        | BuiltinValue::SubgroupSize => None,
    }
}

fn shorthand_scalar(suffix: u8) -> Option<Ty> {
    match suffix {
        b'f' => Some(Ty::F32),
        b'h' => Some(Ty::F16),
        b'i' => Some(Ty::I32),
        b'u' => Some(Ty::U32),
        _ => None,
    }
}

fn constructor_name(ty: &Ty) -> Option<String> {
    match ty {
        Ty::Bool => Some("bool".to_owned()),
        Ty::I32 => Some("i32".to_owned()),
        Ty::U32 => Some("u32".to_owned()),
        Ty::F32 => Some("f32".to_owned()),
        Ty::F16 => Some("f16".to_owned()),
        Ty::Vector(size, _) => Some(format!("vec{size}")),
        Ty::Matrix(columns, rows, _) => Some(format!("mat{columns}x{rows}")),
        Ty::Array(_, _) => Some("array".to_owned()),
        _ => None,
    }
}

fn constructor_templates(ty: &Ty) -> Option<Vec<TpltParam>> {
    let template = match ty {
        Ty::Vector(_, element) | Ty::Matrix(_, _, element) => {
            vec![TpltParam::Type(element.to_wgsl_type()?)]
        }
        Ty::Array(element, size) => {
            let mut template = vec![TpltParam::Type(element.to_wgsl_type()?)];
            if let Some(size) = size {
                template.push(TpltParam::Instance(
                    LiteralInstance::AbstractInt(i64::from(*size)).into(),
                ));
            }
            template
        }
        _ => return None,
    };
    Some(template)
}

fn constructor_shape_valid(constructor: &Ty, arguments: &[Ty]) -> bool {
    match (constructor, arguments) {
        (Ty::Vector(expected, _), [Ty::Vector(actual, _)]) => expected == actual,
        (
            Ty::Matrix(expected_columns, expected_rows, _),
            [Ty::Matrix(actual_columns, actual_rows, _)],
        ) => expected_columns == actual_columns && expected_rows == actual_rows,
        _ => true,
    }
}

fn const_u32(module: &TranslationUnit, expression: &ExpressionNode) -> Option<u32> {
    let (result, _) = wesl::eval(expression.node(), module);
    match result.ok()? {
        wesl::eval::Instance::Literal(LiteralInstance::AbstractInt(value)) => value.try_into().ok(),
        wesl::eval::Instance::Literal(LiteralInstance::I32(value)) => value.try_into().ok(),
        wesl::eval::Instance::Literal(LiteralInstance::U32(value)) => Some(value),
        _ => None,
    }
}

fn member_type(base: Ty, name: &str) -> Option<Ty> {
    match base {
        Ty::Struct(_, fields) => fields
            .into_iter()
            .find_map(|(field, ty)| (field == name).then_some(ty)),
        Ty::Vector(size, element)
            if is_swizzle(name)
                && name.bytes().all(|component| match component {
                    b'x' | b'r' => size >= 1,
                    b'y' | b'g' => size >= 2,
                    b'z' | b'b' => size >= 3,
                    b'w' | b'a' => size >= 4,
                    _ => false,
                }) =>
        {
            Some(if name.len() == 1 {
                *element
            } else {
                Ty::Vector(name.len() as u8, element)
            })
        }
        Ty::Pointer(inner) => member_type(*inner, name),
        Ty::Unknown => Some(Ty::Unknown),
        _ => None,
    }
}

fn index_result(base: Ty) -> Ty {
    match base {
        Ty::Array(element, _) | Ty::Vector(_, element) => *element,
        Ty::Matrix(_, rows, element) => Ty::Vector(rows, element),
        Ty::Pointer(inner) => index_result(*inner),
        _ => Ty::Unknown,
    }
}

fn fill_constructor_type(constructor: Ty, arguments: &[Ty]) -> Ty {
    match constructor {
        Ty::Vector(size, element) if element.is_unknown() => {
            let element = arguments
                .iter()
                .find_map(|argument| match argument {
                    Ty::Vector(_, element) => Some((**element).clone()),
                    scalar if scalar.is_numeric_scalar() || scalar.is_bool() => {
                        Some(scalar.clone())
                    }
                    _ => None,
                })
                .unwrap_or(Ty::Unknown);
            Ty::Vector(size, Box::new(element))
        }
        Ty::Matrix(columns, rows, element) if element.is_unknown() => {
            let element = arguments
                .iter()
                .find_map(|argument| argument.element().cloned())
                .unwrap_or(Ty::AbstractFloat);
            Ty::Matrix(columns, rows, Box::new(element))
        }
        Ty::Array(element, None) if element.is_unknown() => {
            let element = arguments.first().cloned().unwrap_or(Ty::Unknown);
            Ty::Array(Box::new(element), Some(arguments.len() as u32))
        }
        other => other,
    }
}
fn unify_numeric(left: Ty, right: Ty) -> Ty {
    if left.is_unknown() || right.is_unknown() {
        return Ty::Unknown;
    }
    match (&left, &right) {
        (Ty::Vector(_, _), scalar) if scalar.is_numeric_scalar() => return left,
        (scalar, Ty::Vector(_, _)) if scalar.is_numeric_scalar() => return right,
        (Ty::Matrix(_, _, _), scalar) if scalar.is_numeric_scalar() => return left,
        (scalar, Ty::Matrix(_, _, _)) if scalar.is_numeric_scalar() => return right,
        _ => {}
    }
    if left.conversion_rank_to(&right).is_some() {
        return right;
    }
    if right.conversion_rank_to(&left).is_some() {
        return left;
    }
    Ty::Unknown
}

enum BuiltinResolution {
    Match(Option<WgslType>),
    NoMatch(String),
    Unknown,
}

fn builtin_conversion_candidates(ty: &Ty) -> Vec<(u8, WgslType)> {
    let mut candidates = Vec::new();
    if let Some(exact) = ty.to_wgsl_type() {
        candidates.push((0, exact));
    }
    match ty {
        Ty::AbstractInt => {
            candidates.extend([
                (1, WgslType::AbstractFloat),
                (1, WgslType::I32),
                (1, WgslType::U32),
                (2, WgslType::F32),
                (2, WgslType::F16),
            ]);
        }
        Ty::AbstractFloat => {
            candidates.extend([(1, WgslType::F32), (1, WgslType::F16)]);
        }
        Ty::Vector(size, element) => {
            for (rank, element) in builtin_conversion_candidates(element) {
                let candidate = WgslType::Vec(*size, Box::new(element));
                if !candidates
                    .iter()
                    .any(|(_, existing)| existing == &candidate)
                {
                    candidates.push((rank, candidate));
                }
            }
        }
        Ty::Matrix(columns, rows, element) => {
            for (rank, element) in builtin_conversion_candidates(element) {
                let candidate = WgslType::Mat(*columns, *rows, Box::new(element));
                if !candidates
                    .iter()
                    .any(|(_, existing)| existing == &candidate)
                {
                    candidates.push((rank, candidate));
                }
            }
        }
        Ty::Array(element, size) => {
            for (rank, element) in builtin_conversion_candidates(element) {
                let candidate = WgslType::Array(Box::new(element), size.map(|size| size as usize));
                if !candidates
                    .iter()
                    .any(|(_, existing)| existing == &candidate)
                {
                    candidates.push((rank, candidate));
                }
            }
        }
        _ => {}
    }
    candidates
}

fn resolve_builtin_overload(
    name: &str,
    templates: Option<&[TpltParam]>,
    arguments: &[Ty],
) -> BuiltinResolution {
    struct Search {
        best: Option<(u16, Option<WgslType>)>,
        ambiguous: bool,
        error: Option<String>,
        implemented: bool,
    }

    fn visit(
        name: &str,
        templates: Option<&[TpltParam]>,
        candidates: &[Vec<(u8, WgslType)>],
        index: usize,
        rank: u16,
        arguments: &mut Vec<WgslType>,
        search: &mut Search,
    ) {
        if search.best.as_ref().is_some_and(|(best, _)| rank > *best) {
            return;
        }
        if index == candidates.len() {
            match type_builtin_fn(name, templates, arguments) {
                Ok(result) => match &search.best {
                    None => search.best = Some((rank, result)),
                    Some((best_rank, best_result)) if rank < *best_rank => {
                        search.best = Some((rank, result));
                        search.ambiguous = false;
                    }
                    Some((best_rank, best_result))
                        if rank == *best_rank && best_result != &result =>
                    {
                        search.ambiguous = true;
                    }
                    _ => {}
                },
                Err(WgslTypeError::Todo(_)) => {}
                Err(error) => {
                    search.implemented = true;
                    search.error.get_or_insert_with(|| error.to_string());
                }
            }
            return;
        }
        for (conversion_rank, argument) in &candidates[index] {
            arguments.push(argument.clone());
            visit(
                name,
                templates,
                candidates,
                index + 1,
                rank + u16::from(*conversion_rank),
                arguments,
                search,
            );
            arguments.pop();
        }
    }

    let candidates = arguments
        .iter()
        .map(builtin_conversion_candidates)
        .collect::<Vec<_>>();
    if candidates.iter().any(Vec::is_empty) {
        return BuiltinResolution::Unknown;
    }
    let mut search = Search {
        best: None,
        ambiguous: false,
        error: None,
        implemented: false,
    };
    visit(
        name,
        templates,
        &candidates,
        0,
        0,
        &mut Vec::with_capacity(arguments.len()),
        &mut search,
    );
    if search.ambiguous {
        BuiltinResolution::NoMatch("ambiguous automatic conversions".to_owned())
    } else if let Some((_, result)) = search.best {
        BuiltinResolution::Match(result)
    } else if search.implemented {
        BuiltinResolution::NoMatch(
            search
                .error
                .unwrap_or_else(|| "arguments do not satisfy any overload".to_owned()),
        )
    } else {
        BuiltinResolution::Unknown
    }
}

fn assignment_binary_operator(operator: AssignmentOperator) -> Option<BinaryOperator> {
    match operator {
        AssignmentOperator::Equal => None,
        AssignmentOperator::PlusEqual => Some(BinaryOperator::Addition),
        AssignmentOperator::MinusEqual => Some(BinaryOperator::Subtraction),
        AssignmentOperator::TimesEqual => Some(BinaryOperator::Multiplication),
        AssignmentOperator::DivisionEqual => Some(BinaryOperator::Division),
        AssignmentOperator::ModuloEqual => Some(BinaryOperator::Remainder),
        AssignmentOperator::AndEqual => Some(BinaryOperator::BitwiseAnd),
        AssignmentOperator::OrEqual => Some(BinaryOperator::BitwiseOr),
        AssignmentOperator::XorEqual => Some(BinaryOperator::BitwiseXor),
        AssignmentOperator::ShiftRightAssign => Some(BinaryOperator::ShiftRight),
        AssignmentOperator::ShiftLeftAssign => Some(BinaryOperator::ShiftLeft),
    }
}

fn infer_binary_result(operator: BinaryOperator, left: Ty, right: Ty) -> Ty {
    let compatible =
        left.conversion_rank_to(&right).is_some() || right.conversion_rank_to(&left).is_some();
    match operator {
        BinaryOperator::ShortCircuitOr | BinaryOperator::ShortCircuitAnd => {
            if left.is_bool() && right.is_bool() {
                Ty::Bool
            } else {
                Ty::Unknown
            }
        }
        BinaryOperator::Equality | BinaryOperator::Inequality => {
            if !compatible {
                Ty::Unknown
            } else if let Ty::Vector(size, _) = left {
                Ty::Vector(size, Box::new(Ty::Bool))
            } else {
                Ty::Bool
            }
        }
        BinaryOperator::LessThan
        | BinaryOperator::LessThanEqual
        | BinaryOperator::GreaterThan
        | BinaryOperator::GreaterThanEqual => {
            if !left.is_numeric() || !right.is_numeric() || !compatible {
                Ty::Unknown
            } else if let Ty::Vector(size, _) = left {
                Ty::Vector(size, Box::new(Ty::Bool))
            } else {
                Ty::Bool
            }
        }
        BinaryOperator::ShiftLeft | BinaryOperator::ShiftRight => {
            if left.is_integer() && right.is_integer() {
                left
            } else {
                Ty::Unknown
            }
        }
        BinaryOperator::BitwiseOr | BinaryOperator::BitwiseAnd | BinaryOperator::BitwiseXor => {
            let bools = matches!((&left, &right), (Ty::Bool, Ty::Bool))
                || matches!(
                    (&left, &right),
                    (Ty::Vector(_, left), Ty::Vector(_, right))
                        if left.is_bool() && right.is_bool()
                );
            if compatible && (bools || left.is_integer() && right.is_integer()) {
                unify_numeric(left, right)
            } else {
                Ty::Unknown
            }
        }
        BinaryOperator::Multiplication if left.is_numeric() && right.is_numeric() => {
            infer_multiplication(left, right)
        }
        BinaryOperator::Addition
        | BinaryOperator::Subtraction
        | BinaryOperator::Division
        | BinaryOperator::Remainder => {
            if left.is_numeric() && right.is_numeric() {
                unify_numeric(left, right)
            } else {
                Ty::Unknown
            }
        }
        BinaryOperator::Multiplication => Ty::Unknown,
    }
}

fn infer_multiplication(left: Ty, right: Ty) -> Ty {
    match (left, right) {
        (Ty::Matrix(columns, rows, element), Ty::Vector(size, vector_element))
            if columns == size =>
        {
            Ty::Vector(rows, Box::new(unify_numeric(*element, *vector_element)))
        }
        (Ty::Vector(size, vector_element), Ty::Matrix(columns, rows, element)) if size == rows => {
            Ty::Vector(columns, Box::new(unify_numeric(*vector_element, *element)))
        }
        (
            Ty::Matrix(left_columns, left_rows, left_element),
            Ty::Matrix(right_columns, right_rows, right_element),
        ) if left_columns == right_rows => Ty::Matrix(
            right_columns,
            left_rows,
            Box::new(unify_numeric(*left_element, *right_element)),
        ),
        (left, right) => unify_numeric(left, right),
    }
}

fn is_swizzle(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 4
        && (name.bytes().all(|byte| b"xyzw".contains(&byte))
            || name.bytes().all(|byte| b"rgba".contains(&byte)))
}

#[cfg(test)]
mod tests {
    use wgsl_parse::parse_str;

    use super::check_module;

    #[test]
    fn infers_expression_from_editor_local_types() {
        let module = parse_str("fn f() {}").unwrap();
        let expression = "v".parse().unwrap();
        let local_types =
            std::collections::HashMap::from([("v".to_owned(), "vec3<f32>".to_owned())]);
        assert_eq!(
            super::infer_expression_type(
                &module,
                expression,
                &local_types,
                super::TypeEnvironment::default(),
            ),
            super::Ty::Vector(3, Box::new(super::Ty::F32))
        );
    }

    fn naga_accepts(source: &str) -> bool {
        let Ok(module) = naga::front::wgsl::parse_str(source) else {
            return false;
        };
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .is_ok()
    }

    #[test]
    fn reports_explicit_vector_scalar_mismatch() {
        let module = parse_str("fn main() { let x: f32 = vec3(1.0); }").unwrap();
        let diagnostics = check_module(&module);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(
            diagnostics[0].message,
            "type mismatch: expected f32, found vec3<f32>"
        );
    }

    #[test]
    fn unknown_values_poison_without_cascades() {
        let module = parse_str(
            "import package::missing::value; fn main() { let x: f32 = value; let y = x + value; }",
        )
        .unwrap();
        assert!(check_module(&module).is_empty());
    }

    #[test]
    fn curated_wgsl_type_matrix() {
        let valid = [
            "fn f() { let x: f32 = 1.0 + 2.0; }",
            "fn f() { let x: i32 = 1 + 2; }",
            "fn f() { let x: u32 = 1u + 2u; }",
            "fn f() { let x: bool = true && false; }",
            "fn f() { let x: vec3<f32> = vec3(1.0) + vec3(2.0); }",
            "fn f() { let x: vec3<f32> = vec3(1.0) * 2.0; }",
            "fn f() { let x: vec3<f32> = 2.0 * vec3(1.0); }",
            "fn f() { let x: f32 = dot(vec3(1.0), vec3(2.0)); }",
            "fn f() { let x: vec2<f32> = vec3(1.0).xy; }",
            "struct S { x: f32, } fn f(s: S) { let x: f32 = s.x; }",
            "fn g(x: f32) -> f32 { return x; } fn f() { let x: f32 = g(1.0); }",
            "fn f() { var x: f32 = 1.0; x = 2.0; }",
            "@vertex fn f(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f { return vec4f(f32(i)); }",
            "const N = 1u + 1u; @compute @workgroup_size(N) fn f() { var values: array<f32, N>; values[0] = 1.0; }",
        ];
        for source in valid {
            let module = parse_str(source).unwrap();
            assert!(check_module(&module).is_empty(), "{source}");
            assert!(naga_accepts(source), "naga rejected {source}");
        }

        let invalid = [
            "fn f() { let x: f32 = vec3(1.0); }",
            "fn f() { let x: bool = 1; }",
            "fn f() -> f32 { return vec2(1.0); }",
            "fn f() { var x: f32 = 1.0; x = vec2(1.0); }",
            "fn f() { let x: vec2<f32> = vec3(1.0); }",
            "fn f() { let x: i32 = true; }",
            "fn f() { let x: u32 = 1.0; }",
            "fn f() { let x: vec4<f32> = vec2(1.0); }",
            "fn f() { let x: f32 = vec2(1.0).x; let y: bool = x; }",
            "fn f() { let x: bool = vec2(1.0).x; }",
            "fn f() { let x = sin(true); }",
            "fn f() { let x = dot(vec3(1.0), vec2(1.0)); }",
            "fn f() { let x = select(1.0, 2.0, 1.0); }",
            "fn f() { let x = true + false; }",
            "fn f() { let x = ~1.0; }",
            "fn f() { let x = 1.0 << 1; }",
            "fn f() { let x = vec4(vec2(1.0)); }",
            "struct S { x: f32, } fn f() { let x = S(true); }",
            "fn f() { let x = vec3<f32>(1.0, true, 2.0); }",
            "fn f() { var x = true; x += true; }",
            "fn f() { var x: f32 = 1.0; x &= 1.0; }",
            "fn f() { var x: i32 = 1; x += 1.0; }",
            "@vertex fn f() -> @builtin(position) bool { return true; }",
            "@compute @workgroup_size(0) fn f() {}",
        ];
        for source in invalid {
            let module = parse_str(source).unwrap();
            assert!(!check_module(&module).is_empty(), "checker missed {source}");
            assert!(!naga_accepts(source), "naga accepted {source}");
        }
    }

    #[test]
    fn accepts_struct_member_assignments() {
        let source = r#"
struct VsIn {
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
}
@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    let ndc = vec2<f32>(in.uv.x, 1.0 - in.uv.y);
    out.clip = vec4<f32>(ndc, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    return out;
}
"#;
        let module = parse_str(source).unwrap();
        let diagnostics = check_module(&module);
        assert!(diagnostics.is_empty(), "{diagnostics:#?}");
        assert!(naga_accepts(source));
    }
}
