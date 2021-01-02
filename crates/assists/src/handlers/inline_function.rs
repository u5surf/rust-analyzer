use std::fmt::Write as _;

use hir::{HasSource, HasVisibility, ModuleDef, PathResolution};
use syntax::{
    ast::{self, ArgListOwner},
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
//     let x = { let a = 1; let b = 2; a + b };
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
    let body = function_source.value.body()?.expr()?;
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
            let mut buffer = String::new();

            writeln!(buffer, "{{").expect("never fails");

            for (pattern, value) in new_bindings {
                writeln!(buffer, "let {} = {};", pattern, value).expect("never fails");
            }

            writeln!(buffer, "{}", body).expect("never fails");
            buffer.push_str("}");

            builder.replace(target, buffer);
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
