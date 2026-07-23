use std::{collections::HashMap, fmt, ops::Range};

use wgsl_parse::syntax::{
    AssignmentOperator, Attribute, BinaryOperator, CaseSelector, CompoundStatement, Declaration,
    DeclarationKind, Expression, ExpressionNode, FunctionCall, GlobalDeclaration,
    LiteralExpression, Statement, StatementNode, TranslationUnit, TypeExpression, UnaryOperator,
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

#[derive(Default)]
struct ModuleTypes {
    named: HashMap<String, Ty>,
    globals: HashMap<String, Ty>,
    functions: HashMap<String, FunctionTy>,
}

pub fn check_module(module: &TranslationUnit) -> Vec<TypeDiagnostic> {
    let mut checker = Checker {
        module,
        types: ModuleTypes::default(),
        diagnostics: Vec::new(),
    };
    checker.collect_types();
    checker.check_globals();
    checker.check_functions();
    checker.check_const_asserts();
    checker.diagnostics
}

struct Checker<'a> {
    module: &'a TranslationUnit,
    types: ModuleTypes,
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
                    let fields = structure
                        .members
                        .iter()
                        .map(|member| {
                            (
                                member.ident.name().to_string(),
                                self.resolve_type(&member.ty),
                            )
                        })
                        .collect();
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

    fn check_io_attributes(
        &mut self,
        attributes: &[wgsl_parse::syntax::AttributeNode],
        ty: &Ty,
        declaration: Range<usize>,
    ) {
        for attribute in attributes {
            if matches!(attribute.node(), Attribute::Location(_))
                && !ty.is_unknown()
                && !ty.is_numeric_scalar()
                && !matches!(ty, Ty::Vector(_, element) if element.is_numeric_scalar())
            {
                self.diagnostics.push(TypeDiagnostic {
                    range: attribute.span().range(),
                    message: format!(
                        "@location value must be a numeric scalar or vector, found {ty}"
                    ),
                    related: vec![(declaration.clone(), "entry point declared here".to_owned())],
                });
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
                    self.diagnostics.push(TypeDiagnostic {
                        range: expression.span().range(),
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
                        Ty::Vector(size, element) if *element == Ty::Bool => {
                            Ty::Vector(size, element)
                        }
                        Ty::Unknown => Ty::Unknown,
                        other => {
                            self.check_bool(unary.operand.span().range(), &other);
                            Ty::Unknown
                        }
                    },
                    UnaryOperator::AddressOf => Ty::Pointer(Box::new(operand)),
                    UnaryOperator::Indirection => match operand {
                        Ty::Pointer(inner) => *inner,
                        Ty::Unknown => Ty::Unknown,
                        other => {
                            self.diagnostics.push(TypeDiagnostic {
                                range: unary.operand.span().range(),
                                message: format!("cannot dereference {other}"),
                                related: Vec::new(),
                            });
                            Ty::Unknown
                        }
                    },
                    UnaryOperator::Negation | UnaryOperator::BitwiseComplement => operand,
                }
            }
            Expression::Binary(binary) => {
                let left = self.infer_expression(&binary.left, locals);
                let right = self.infer_expression(&binary.right, locals);
                match binary.operator {
                    BinaryOperator::ShortCircuitOr | BinaryOperator::ShortCircuitAnd => {
                        self.check_bool(binary.left.span().range(), &left);
                        self.check_bool(binary.right.span().range(), &right);
                        Ty::Bool
                    }
                    BinaryOperator::Equality
                    | BinaryOperator::Inequality
                    | BinaryOperator::LessThan
                    | BinaryOperator::LessThanEqual
                    | BinaryOperator::GreaterThan
                    | BinaryOperator::GreaterThanEqual => {
                        if right.conversion_rank_to(&left).is_none()
                            && left.conversion_rank_to(&right).is_none()
                        {
                            self.check_compatible(binary.right.span().range(), &right, &left, None);
                        }
                        match left {
                            Ty::Vector(size, _) => Ty::Vector(size, Box::new(Ty::Bool)),
                            Ty::Unknown => Ty::Unknown,
                            _ => Ty::Bool,
                        }
                    }
                    BinaryOperator::ShiftLeft | BinaryOperator::ShiftRight => left,
                    _ => unify_numeric(left, right),
                }
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
        let constructor = self.resolve_type(&call.ty);
        if !constructor.is_unknown() {
            return fill_constructor_type(constructor, &arguments);
        }
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
        if let Some(function) = builtin(&name) {
            let arity_matches = function
                .overloads
                .iter()
                .filter(|overload| signature_accepts_arity(overload.signature, arguments.len()))
                .count();
            if arity_matches == 0
                && function
                    .overloads
                    .iter()
                    .all(|overload| signature_has_fixed_arity(overload.signature))
            {
                self.diagnostics.push(TypeDiagnostic {
                    range,
                    message: format!(
                        "no matching overload for {name} with {} arguments",
                        arguments.len()
                    ),
                    related: Vec::new(),
                });
                return Ty::Unknown;
            }
            return builtin_result(&name, &arguments);
        }
        Ty::Unknown
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
            name if name.starts_with("mat") && name.len() == 6 => {
                let bytes = name.as_bytes();
                Ty::Matrix(
                    bytes[3] - b'0',
                    bytes[5] - b'0',
                    Box::new(self.template_type(ty, 0)),
                )
            }
            "array" => Ty::Array(Box::new(self.template_type(ty, 0)), template_u32(ty, 1)),
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
}

fn template_u32(ty: &TypeExpression, index: usize) -> Option<u32> {
    ty.template_args
        .as_ref()?
        .get(index)
        .and_then(|argument| match argument.expression.node() {
            Expression::Literal(LiteralExpression::AbstractInt(value)) => (*value).try_into().ok(),
            Expression::Literal(LiteralExpression::U32(value)) => Some(*value),
            _ => None,
        })
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
    if left.conversion_rank_to(&right).is_some() {
        return right;
    }
    if right.conversion_rank_to(&left).is_some() {
        return left;
    }
    match (&left, &right) {
        (Ty::Vector(_, _), scalar) if scalar.is_numeric_scalar() => left,
        (scalar, Ty::Vector(_, _)) if scalar.is_numeric_scalar() => right,
        _ => Ty::Unknown,
    }
}

fn builtin_result(name: &str, arguments: &[Ty]) -> Ty {
    let first = arguments.first().cloned().unwrap_or(Ty::Unknown);
    match name {
        "all" | "any" => Ty::Bool,
        "arrayLength" | "textureNumLayers" | "textureNumLevels" | "textureNumSamples" => Ty::U32,
        "dot" | "length" | "distance" | "determinant" => {
            first.element().cloned().unwrap_or(first).concretize()
        }
        "textureDimensions" => Ty::Unknown,
        "textureLoad"
        | "textureSample"
        | "textureSampleBias"
        | "textureSampleCompare"
        | "textureSampleCompareLevel"
        | "textureSampleGrad"
        | "textureSampleLevel" => Ty::Unknown,
        "select" if arguments.len() >= 2 => arguments[0].clone().concretize(),
        _ => Ty::Unknown,
    }
}

fn signature_has_fixed_arity(signature: &str) -> bool {
    !signature.contains("...") && !signature.contains("optional")
}

fn signature_accepts_arity(signature: &str, actual: usize) -> bool {
    let Some(start) = signature.find('(') else {
        return true;
    };
    let Some(end) = signature.rfind(')') else {
        return true;
    };
    let parameters = &signature[start + 1..end];
    if parameters.trim().is_empty() {
        return actual == 0;
    }
    if parameters.contains("...") {
        return true;
    }
    let mut depth = 0;
    let mut count = 1;
    for character in parameters.chars() {
        match character {
            '<' | '(' => depth += 1,
            '>' | ')' => depth -= 1,
            ',' if depth == 0 => count += 1,
            _ => {}
        }
    }
    actual == count
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
        ];
        for source in invalid {
            let module = parse_str(source).unwrap();
            assert!(!check_module(&module).is_empty(), "checker missed {source}");
            assert!(!naga_accepts(source), "naga accepted {source}");
        }
    }
}
