use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::visit_mut::{self, VisitMut};
use syn::{Expr, ExprCall, ExprPath, ItemFn, LitStr, Path, parse_macro_input, parse_quote};

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
    let mut block = function.block;

    hidden_sig.ident = hidden_ident;
    hidden_sig
        .inputs
        .insert(0, parse_quote!(__composer: &mut ::karu::Composer));
    ComposerInjector.visit_block_mut(&mut block);

    quote! {
        #(#attrs)*
        #[allow(non_snake_case)]
        #[allow(dead_code, unused_variables)]
        #vis #original_sig {
        }

        #[doc(hidden)]
        #[allow(non_snake_case)]
        #(#hidden_attrs)*
        #vis #hidden_sig {
            ::karu::__private::with_component_scope(__composer, #name, |__composer| #block)
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
    fn visit_expr_call_mut(&mut self, call: &mut ExprCall) {
        visit_mut::visit_expr_call_mut(self, call);

        let Some(name) = call_name(&call.func) else {
            return;
        };

        match name.as_str() {
            "remember_state" => {
                replace_with_private_call(call, "remember_state");
                inject_composer(call);
            }
            "Column" => {
                rewrite_child_closure(call.args.iter_mut().next());
                replace_with_private_call(call, "Column");
                inject_composer(call);
            }
            "Column_with_modifier" => {
                rewrite_child_closure(call.args.iter_mut().nth(1));
                replace_with_private_call(call, "Column_with_modifier");
                inject_composer(call);
            }
            "Text" => {
                replace_with_private_call(call, "Text");
                inject_composer(call);
            }
            "Text_with_modifier" => {
                replace_with_private_call(call, "Text_with_modifier");
                inject_composer(call);
            }
            _ if is_component_name(&name) => {
                replace_with_hidden_component_call(call, &name);
                inject_composer(call);
            }
            _ => {}
        }
    }
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

fn rewrite_child_closure(child: Option<&mut Expr>) {
    let Some(Expr::Closure(closure)) = child else {
        return;
    };

    if closure.inputs.is_empty() {
        closure.inputs.push(parse_quote!(__composer));
    }
}

fn is_component_name(name: &str) -> bool {
    !matches!(
        name,
        "Some" | "Ok" | "Err" | "Box" | "Rc" | "Arc" | "Cell" | "RefCell" | "Vec" | "String"
    ) && name
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_uppercase())
}
