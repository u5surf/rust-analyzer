use ast::make;
use hir::{HasSource, HasVisibility, ModuleDef, PathResolution};
use syntax::{
    ast::{self, edit::AstNodeEdit, ArgListOwner},
    AstNode,
};

use crate::{
    assist_context::{AssistContext, Assists},
    AssistId, AssistKind,
};

// Assist: inline_function
//
// Inlines a function body.
//
// ```
// fn add(a: u32, b: u32) -> u32 { a + b }
// fn main() {
//     let x = add<|>(1, 2);
// }
// ```
// ->
// ```
// fn add(a: u32, b: u32) -> u32 { a + b }
// fn main() {
//     let x = {
//         let a = 1;
//         let b = 2;
//         a + b
//     };
// }
// ```
pub(crate) fn inline_function(acc: &mut Assists, ctx: &AssistContext) -> Option<()> {
    let path_expr: ast::PathExpr = ctx.find_node_at_offset()?;
    let call = path_expr.syntax().parent().and_then(ast::CallExpr::cast)?;
    let path = path_expr.path()?;

    let function = match ctx.sema.resolve_path(&path)? {
        PathResolution::Def(ModuleDef::Function(f)) => f,
        _ => return None,
    };

    let current_scope = ctx.sema.scope(call.syntax());
    let current_module = current_scope.module()?;

    if !function.is_visible_from(ctx.db(), current_module) {
        // The function isn't accessible from here so we can't inline it
        return None;
    }

    let function_source = function.source(ctx.db());
    let body = function_source.value.body()?;
    let target = call.syntax().text_range();

    let arguments: Vec<_> = call.arg_list()?.args().collect();
    let parameters = function_parameter_patterns(&function_source.value)?;

    if arguments.len() != parameters.len() {
        // They've passed the wrong number of arguments to this function
        return None;
    }

    let new_bindings = parameters.into_iter().zip(arguments);

    acc.add(
        AssistId("inline_function", AssistKind::RefactorInline),
        format!("Inline `{}`", path),
        target,
        |builder| {
            let mut statements: Vec<ast::Stmt> = Vec::new();

            for (pattern, value) in new_bindings {
                statements.push(make::let_stmt(pattern, Some(value)).into());
            }

            statements.extend(body.statements());

            let original_indentation = call.indent_level();
            let replacement = make::block_expr(statements, body.expr())
                .reset_indent()
                .indent(original_indentation);

            builder.replace_ast(ast::Expr::CallExpr(call), ast::Expr::BlockExpr(replacement));
        },
    )
}

fn function_parameter_patterns(value: &ast::Fn) -> Option<Vec<ast::Pat>> {
    let mut patterns = Vec::new();

    for param in value.param_list()?.params() {
        let pattern = param.pat()?;
        patterns.push(pattern);
    }

    Some(patterns)
}

#[cfg(test)]
mod tests {
    use crate::tests::{check_assist, check_assist_not_applicable};

    use super::*;

    #[test]
    fn no_args_or_return_value_gets_inlined_without_block() {
        check_assist(
            inline_function,
            r#"
            fn foo() { println!("Hello, World!"); }
            fn main() {
                fo<|>o();
            }
            "#,
            r#"
            fn foo() { println!("Hello, World!"); }
            fn main() {
                {
                    println!("Hello, World!");
                };
            }
            "#,
        );
    }

    #[test]
    fn args_with_side_effects() {
        check_assist(
            inline_function,
            r#"
            fn foo(name: String) { println!("Hello, {}!", name); }
            fn main() {
                foo<|>(String::from("Michael"));
            }
            "#,
            r#"
            fn foo(name: String) { println!("Hello, {}!", name); }
            fn main() {
                {
                    let name = String::from("Michael");
                    println!("Hello, {}!", name);
                };
            }
            "#,
        );
    }

    #[test]
    #[ignore = "not implemented"]
    fn copy_args_with_no_side_effects_get_inlined() {
        check_assist(
            inline_function,
            r#"
            fn add(a: u32, b: u32) -> u32 { a + b }
            fn main() {
                let x = add<|>(1, 2);
            }
            "#,
            r#"
            fn add(a: u32, b: u32) -> u32 { a + b }
            fn main() {
                let x = 1 + 2;
            }
            "#,
        );
    }

    #[test]
    fn cant_inline_when_the_function_is_inaccessible() {
        check_assist_not_applicable(
            inline_function,
            r#"
            mod private {
                fn add(a: u32, b: u32) -> u32 { a + b }
            }
            fn main() { let x = private::add<|>(1, 2); }
            "#,
        );
    }

    #[test]
    fn method_inlining_isnt_supported() {
        check_assist_not_applicable(
            inline_function,
            r"
            struct Foo;
            impl Foo { fn bar(&self) {} }

            fn main() { Foo.bar<|>(); }
            ",
        );
    }

    #[test]
    fn function_with_multiple_statements() {
        check_assist(
            inline_function,
            r#"
            fn foo(a: u32, b: u32) -> u32 {
                let x = a + b;
                let y = x - b;
                x * y
            }

            fn main() {
                let x = foo<|>(1, 2);
            }
            "#,
            r#"
            fn foo(a: u32, b: u32) -> u32 {
                let x = a + b;
                let y = x - b;
                x * y
            }

            fn main() {
                let x = {
                    let a = 1;
                    let b = 2;
                    let x = a + b;
                    let y = x - b;
                    x * y
                };
            }
            "#,
        );
    }
}
