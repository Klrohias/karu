use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{ItemFn, LitStr, parse_macro_input, parse_quote};

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
    let hidden_block = function.block;

    let arg_names: Vec<_> = original_sig
        .inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Typed(pat_type) => &pat_type.pat,
            syn::FnArg::Receiver(_) => {
                unreachable!("self-receiver composable functions are not supported")
            }
        })
        .collect();

    hidden_sig.ident = hidden_ident.clone();
    hidden_sig
        .inputs
        .insert(0, parse_quote!(__composer: &mut ::karu::Composer));

    quote! {
        #(#attrs)*
        #[allow(non_snake_case)]
        #vis #original_sig {
            ::karu::__private::with_current_composer(|__composer| {
                #hidden_ident(__composer, #(#arg_names),*)
            })
        }

        #[doc(hidden)]
        #[allow(non_snake_case)]
        #(#hidden_attrs)*
        #vis #hidden_sig {
            ::karu::__private::with_component_scope_unit(__composer, #name, |__composer| {
                use ::karu::__private::*;
                #hidden_block
            })
        }
    }
    .into()
}
