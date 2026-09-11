use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{parse_macro_input, ItemFn};

/// Register a safe synchronous zero-arg function via the Suite builder.
/// Enable the `macros` feature and call generated `register_<name>(&mut suite)`.
#[proc_macro_attribute]
pub fn bench(args: TokenStream, item: TokenStream) -> TokenStream {
    let f = parse_macro_input!(item as ItemFn);
    if !args.is_empty()
        || !f.sig.inputs.is_empty()
        || !f.sig.generics.params.is_empty()
        || f.sig.asyncness.is_some()
        || f.sig.unsafety.is_some()
    {
        return syn::Error::new_spanned(
            &f.sig,
            "bench requires a safe synchronous nongeneric function with no arguments; use Suite::bench_batch / bench_with_input for parameterized or batched workloads",
        )
        .to_compile_error()
        .into();
    }
    let name = &f.sig.ident;
    let register = format_ident!("register_{}", name);
    let visibility = &f.vis;
    quote! {
        #f
        #visibility fn #register(suite: &mut ::rbench::Suite<'_>) {
            suite.bench(stringify!(#name), #name);
        }
    }
    .into()
}

/// Expand a Criterion-style harness `main` that builds a Suite and runs it.
///
/// Body registers benches on `suite` (typically via `register_*` helpers).
/// Accepts either no arguments or a single `&mut Suite<'_>`.
#[proc_macro_attribute]
pub fn main(args: TokenStream, item: TokenStream) -> TokenStream {
    let f = parse_macro_input!(item as ItemFn);
    if !args.is_empty() || f.sig.asyncness.is_some() || f.sig.unsafety.is_some() {
        return syn::Error::new_spanned(
            &f.sig,
            "rbench::main requires a safe synchronous function; take `&mut Suite<'_>` or no args",
        )
        .to_compile_error()
        .into();
    }
    if !f.sig.inputs.is_empty() && f.sig.inputs.len() != 1 {
        return syn::Error::new_spanned(
            &f.sig,
            "rbench::main accepts either no arguments or a single `&mut Suite<'_>`",
        )
        .to_compile_error()
        .into();
    }
    let attrs = &f.attrs;
    let vis = &f.vis;
    let body = &f.block;
    quote! {
        #(#attrs)*
        #vis fn main() -> ::rbench::Result<()> {
            let mut suite = ::rbench::Suite::new(env!("CARGO_CRATE_NAME"));
            let __rbench_user = |suite: &mut ::rbench::Suite<'_>| -> ::rbench::Result<()> #body;
            __rbench_user(&mut suite)?;
            suite.main()
        }
    }
    .into()
}
