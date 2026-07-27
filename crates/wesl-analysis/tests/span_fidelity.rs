use std::{fs, path::PathBuf};

use wesl_analysis::{LineIndex, PositionEncoding};
use wgsl_parse::{
    parse_str,
    syntax::{
        Attribute, Attributes, CaseSelector, CompoundStatement, Declaration, Expression,
        ExpressionNode, FunctionCall, GlobalDeclaration, Statement, StatementNode, TypeExpression,
    },
};

struct SpanProbe<'a> {
    source: &'a str,
    lines: LineIndex,
    identifiers: usize,
}

impl<'a> SpanProbe<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            lines: LineIndex::new(source, PositionEncoding::default()),
            identifiers: 0,
        }
    }

    fn assert_span(&self, start: usize, end: usize) {
        assert!(start < end, "empty span at {start}");
        assert!(
            end <= self.source.len(),
            "span {start}..{end} is out of bounds"
        );
        let start_position = self.lines.offset_to_position(self.source, start).unwrap();
        let end_position = self.lines.offset_to_position(self.source, end).unwrap();
        assert_eq!(
            self.lines
                .position_to_offset(self.source, start_position)
                .unwrap(),
            start
        );
        assert_eq!(
            self.lines
                .position_to_offset(self.source, end_position)
                .unwrap(),
            end
        );
    }

    fn visit_attributes(&mut self, attributes: &Attributes) {
        for attribute in attributes {
            self.assert_span(attribute.span().start, attribute.span().end);
            match attribute.node() {
                Attribute::Align(expr)
                | Attribute::Binding(expr)
                | Attribute::BlendSrc(expr)
                | Attribute::Group(expr)
                | Attribute::Id(expr)
                | Attribute::Location(expr)
                | Attribute::Size(expr)
                | Attribute::If(expr)
                | Attribute::Elif(expr) => self.visit_expression(expr),
                Attribute::WorkgroupSize(size) => {
                    self.visit_expression(&size.x);
                    if let Some(expr) = &size.y {
                        self.visit_expression(expr);
                    }
                    if let Some(expr) = &size.z {
                        self.visit_expression(expr);
                    }
                }
                Attribute::Custom(custom) => {
                    if let Some(arguments) = &custom.arguments {
                        for argument in arguments {
                            self.visit_expression(argument);
                        }
                    }
                }
                Attribute::Builtin(_)
                | Attribute::Const
                | Attribute::Diagnostic(_)
                | Attribute::Else
                | Attribute::Fragment
                | Attribute::Interpolate(_)
                | Attribute::Invariant
                | Attribute::MustUse
                | Attribute::Publish
                | Attribute::Vertex
                | Attribute::Compute => {}
            }
        }
    }

    fn visit_type(&mut self, ty: &TypeExpression) {
        if let Some(arguments) = &ty.template_args {
            for argument in arguments {
                self.visit_expression(&argument.expression);
            }
        }
    }

    fn visit_call(&mut self, call: &FunctionCall) {
        self.visit_type(&call.ty);
        for argument in &call.arguments {
            self.visit_expression(argument);
        }
    }

    fn visit_expression(&mut self, expression: &ExpressionNode) {
        let span = expression.span();
        self.assert_span(span.start, span.end);
        match expression.node() {
            Expression::Literal(_) => {}
            Expression::Parenthesized(expr) => self.visit_expression(&expr.expression),
            Expression::NamedComponent(expr) => self.visit_expression(&expr.base),
            Expression::Indexing(expr) => {
                self.visit_expression(&expr.base);
                self.visit_expression(&expr.index);
            }
            Expression::Unary(expr) => self.visit_expression(&expr.operand),
            Expression::Binary(expr) => {
                self.visit_expression(&expr.left);
                self.visit_expression(&expr.right);
            }
            Expression::FunctionCall(call) => self.visit_call(call),
            Expression::TypeOrIdentifier(ty) => {
                self.visit_type(ty);
                if ty.path.is_none() && ty.template_args.is_none() {
                    self.identifiers += 1;
                    assert_eq!(
                        self.source[span.range()].trim(),
                        ty.ident.name().as_str(),
                        "identifier expression span did not isolate the identifier"
                    );
                }
            }
        }
    }

    fn visit_declaration(&mut self, declaration: &Declaration) {
        self.visit_attributes(&declaration.attributes);
        if let Some(ty) = &declaration.ty {
            self.visit_type(ty);
        }
        if let Some(initializer) = &declaration.initializer {
            self.visit_expression(initializer);
        }
    }

    fn visit_compound(&mut self, compound: &CompoundStatement) {
        self.visit_attributes(&compound.attributes);
        for statement in &compound.statements {
            self.visit_statement(statement);
        }
    }

    fn visit_statement(&mut self, statement: &StatementNode) {
        let span = statement.span();
        self.assert_span(span.start, span.end);
        match statement.node() {
            Statement::Void
            | Statement::Break(_)
            | Statement::Continue(_)
            | Statement::Discard(_) => {}
            Statement::Compound(compound) => self.visit_compound(compound),
            Statement::Assignment(assign) => {
                self.visit_expression(&assign.lhs);
                self.visit_expression(&assign.rhs);
            }
            Statement::Increment(increment) => self.visit_expression(&increment.expression),
            Statement::Decrement(decrement) => self.visit_expression(&decrement.expression),
            Statement::If(if_statement) => {
                self.visit_attributes(&if_statement.attributes);
                self.visit_expression(&if_statement.if_clause.expression);
                self.visit_compound(&if_statement.if_clause.body);
                for clause in &if_statement.else_if_clauses {
                    self.visit_expression(&clause.expression);
                    self.visit_compound(&clause.body);
                }
                if let Some(clause) = &if_statement.else_clause {
                    self.visit_compound(&clause.body);
                }
            }
            Statement::Switch(switch) => {
                self.visit_attributes(&switch.attributes);
                self.visit_expression(&switch.expression);
                for clause in &switch.clauses {
                    for selector in &clause.case_selectors {
                        if let CaseSelector::Expression(expression) = selector {
                            self.visit_expression(expression);
                        }
                    }
                    self.visit_compound(&clause.body);
                }
            }
            Statement::Loop(loop_statement) => {
                self.visit_attributes(&loop_statement.attributes);
                self.visit_compound(&loop_statement.body);
                if let Some(continuing) = &loop_statement.continuing {
                    self.visit_compound(&continuing.body);
                    if let Some(break_if) = &continuing.break_if {
                        self.visit_expression(&break_if.expression);
                    }
                }
            }
            Statement::For(for_statement) => {
                self.visit_attributes(&for_statement.attributes);
                if let Some(initializer) = &for_statement.initializer {
                    self.visit_statement(initializer);
                }
                if let Some(condition) = &for_statement.condition {
                    self.visit_expression(condition);
                }
                if let Some(update) = &for_statement.update {
                    self.visit_statement(update);
                }
                self.visit_compound(&for_statement.body);
            }
            Statement::While(while_statement) => {
                self.visit_attributes(&while_statement.attributes);
                self.visit_expression(&while_statement.condition);
                self.visit_compound(&while_statement.body);
            }
            Statement::Return(return_statement) => {
                if let Some(expression) = &return_statement.expression {
                    self.visit_expression(expression);
                }
            }
            Statement::FunctionCall(call) => self.visit_call(&call.call),
            Statement::ConstAssert(assertion) => self.visit_expression(&assertion.expression),
            Statement::Declaration(declaration) => self.visit_declaration(declaration),
        }
    }
}

fn terrain_path() -> Option<PathBuf> {
    if let Some(root) = std::env::var_os("WESL_LSP_PRIVATE_CORPUS") {
        let candidate = PathBuf::from(root).join("terrain.wesl");
        // Unlike the fallback path below, the operator explicitly pointed us here, so a
        // missing file is a misconfiguration to report, not a reason to skip quietly.
        assert!(
            candidate.exists(),
            "WESL_LSP_PRIVATE_CORPUS is set but {} is missing",
            candidate.display()
        );
        return Some(candidate);
    }
    let local = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../Seclorum/novus/crates/novus-render/shaders/terrain.wesl");
    local.exists().then_some(local)
}

#[test]
fn terrain_has_faithful_declaration_and_identifier_spans() {
    let Some(path) = terrain_path() else {
        eprintln!("skipping private terrain span probe");
        return;
    };
    let source = fs::read_to_string(path).unwrap();
    let module = parse_str(&source).unwrap();
    let mut probe = SpanProbe::new(&source);

    assert!(!module.global_declarations.is_empty());
    for declaration in &module.global_declarations {
        let span = declaration.span();
        probe.assert_span(span.start, span.end);
        match declaration.node() {
            GlobalDeclaration::Void => {}
            GlobalDeclaration::Declaration(declaration) => probe.visit_declaration(declaration),
            GlobalDeclaration::TypeAlias(alias) => {
                probe.visit_attributes(&alias.attributes);
                probe.visit_type(&alias.ty);
            }
            GlobalDeclaration::Struct(structure) => {
                probe.visit_attributes(&structure.attributes);
                for member in &structure.members {
                    probe.assert_span(member.span().start, member.span().end);
                    probe.visit_attributes(&member.attributes);
                    probe.visit_type(&member.ty);
                }
            }
            GlobalDeclaration::Function(function) => {
                probe.visit_attributes(&function.attributes);
                for parameter in &function.parameters {
                    probe.visit_attributes(&parameter.attributes);
                    probe.visit_type(&parameter.ty);
                }
                probe.visit_attributes(&function.return_attributes);
                if let Some(return_type) = &function.return_type {
                    probe.visit_type(return_type);
                }
                probe.visit_compound(&function.body);
            }
            GlobalDeclaration::ConstAssert(assertion) => {
                probe.visit_expression(&assertion.expression)
            }
        }
    }

    assert!(
        probe.identifiers > 100,
        "unexpectedly few identifier expressions"
    );
}

#[test]
fn syntax_errors_return_no_partial_ast() {
    let broken = "const before = 1;\nfn broken( {\nconst after = 2;\n";
    assert!(parse_str(broken).is_err());
}
