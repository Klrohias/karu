use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::visit_mut::{self, VisitMut};
use syn::{Expr, ExprCall, ExprPath, ItemFn, LitStr, Path, Stmt, parse_macro_input, parse_quote};

#[proc_macro_attribute]
pub fn composable(_attr: TokenStream, input: TokenStream) -> TokenStream {
    let function = parse_macro_input!(input as ItemFn);

    if function.sig.asyncness.is_some() {
        return syn::Error::new_spanned(
            function.sig.asyncness,
            "#[composable] does not support async functions yet",
        )
        .to_compile_error()
        .into();
    }

    let attrs = function.attrs;
    let hidden_attrs = attrs.clone();
    let vis = function.vis;
    let original_sig = function.sig;
    let hidden_ident = format_ident!("__karu_{}", original_sig.ident);
    let name = LitStr::new(&original_sig.ident.to_string(), original_sig.ident.span());
    let mut hidden_sig = original_sig.clone();
    let original_block = function.block.clone();
    let mut hidden_block = function.block;

    hidden_sig.ident = hidden_ident;
    hidden_sig
        .inputs
        .insert(0, parse_quote!(__composer: &mut ::karu::Composer));
    ComposerInjector.visit_block_mut(&mut hidden_block);

    quote! {
        #(#attrs)*
        #[allow(non_snake_case)]
        #[allow(dead_code, unused_variables)]
        #vis #original_sig #original_block

        #[doc(hidden)]
        #[allow(non_snake_case)]
        #(#hidden_attrs)*
        #vis #hidden_sig {
            ::karu::__private::with_component_scope(__composer, #name, |__composer| {
                use ::karu::__private::*;
                #hidden_block
            })
        }
    }
    .into()
}

#[proc_macro]
pub fn mangled_composable(input: TokenStream) -> TokenStream {
    let mut path = parse_macro_input!(input as Path);
    let Some(segment) = path.segments.last_mut() else {
        return syn::Error::new_spanned(path, "expected a composable function path")
            .to_compile_error()
            .into();
    };

    segment.ident = format_ident!("__karu_{}", segment.ident);
    quote!(#path).into()
}

struct ComposerInjector;

impl VisitMut for ComposerInjector {
    fn visit_stmt_mut(&mut self, stmt: &mut Stmt) {
        match stmt {
            Stmt::Expr(expr, Some(_)) => {
                visit_mut::visit_expr_mut(self, expr);
                rewrite_composable_statement(expr);
            }
            _ => visit_mut::visit_stmt_mut(self, stmt),
        }
    }

    fn visit_expr_call_mut(&mut self, call: &mut ExprCall) {
        visit_mut::visit_expr_call_mut(self, call);

        let Some(name) = call_name(&call.func) else {
            return;
        };

        if name == "remember_state" {
            replace_with_private_call(call, "remember_state");
            inject_composer(call);
        }
    }
}

fn rewrite_composable_statement(expr: &mut Expr) {
    let Expr::Call(call) = expr else {
        return;
    };

    let Some(name) = call_name(&call.func) else {
        return;
    };

    if !is_composable_name(&name) {
        return;
    }

    rewrite_child_closures(call);
    replace_with_hidden_component_call(call, &name);
    inject_composer(call);
}

fn call_name(func: &Expr) -> Option<String> {
    let Expr::Path(ExprPath {
        qself: None, path, ..
    }) = func
    else {
        return None;
    };

    path.segments
        .last()
        .map(|segment| segment.ident.to_string())
}

fn replace_with_private_call(call: &mut ExprCall, function: &str) {
    let function = format_ident!("{function}");
    call.func = Box::new(parse_quote!(::karu::__private::#function));
}

fn inject_composer(call: &mut ExprCall) {
    call.args.insert(0, parse_quote!(__composer));
}

fn replace_with_hidden_component_call(call: &mut ExprCall, name: &str) {
    let Expr::Path(ExprPath { path, .. }) = call.func.as_mut() else {
        return;
    };

    if let Some(segment) = path.segments.last_mut() {
        segment.ident = format_ident!("__karu_{name}");
    }
}

fn rewrite_child_closures(call: &mut ExprCall) {
    for arg in call.args.iter_mut() {
        rewrite_child_closure(arg);
    }
}

fn rewrite_child_closure(child: &mut Expr) {
    let Expr::Closure(closure) = child else {
        return;
    };

    if closure.inputs.is_empty() {
        closure.inputs.push(parse_quote!(__composer));
    }
}

fn is_composable_name(name: &str) -> bool {
    !matches!(name, "Some" | "Ok" | "Err") && is_upper_camel_case(name)
}

fn is_upper_camel_case(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
        && chars.all(|ch| ch.is_ascii_alphanumeric())
}
