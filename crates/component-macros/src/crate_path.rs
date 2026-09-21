use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::{Ident, Span, TokenStream};
use quote::quote;

/// Resolve the GPUI API path for the crate where a macro is expanded.
///
/// The dependency name resolved from the manifest is preferred, so renames in
/// `gpui-kit` and standalone `gpui-component` consumers are honored. When no
/// manifest is available -- a non-Cargo build such as buck or bazel, where
/// `proc_macro_crate` has no manifest to read -- fall back to the conventional
/// `gpui` extern crate name, which is how GPUI is normally imported.
pub(crate) fn gpui() -> TokenStream {
    resolved_path(&["gpui-kit", "gpui-pre"]).unwrap_or_else(|| extern_path("gpui"))
}

/// Resolve the `gpui-component` API path, mirroring [`gpui`]: `gpui-kit`
/// consumers reach it as `gpui_kit::component`, standalone consumers as
/// `gpui_component`, and the crate itself as `crate`.
pub(crate) fn component() -> TokenStream {
    if let Ok(found) = crate_name("gpui-kit") {
        let kit = found_crate_path(found);
        return quote!(#kit::component);
    }
    if let Ok(found) = crate_name("gpui-component") {
        return found_crate_path(found);
    }
    // No manifest dependency resolved (a non-cargo build such as buck): `crate`
    // when compiling `gpui-component` itself, otherwise the same root as GPUI,
    // since the gpui-kit umbrella that re-exports GPUI there also re-exports the
    // component layer as `<gpui>::component`.
    if is_self("gpui-component") {
        quote!(crate)
    } else {
        let gpui = gpui();
        quote!(#gpui::component)
    }
}

/// Path of the first candidate package that resolves to a manifest dependency.
fn resolved_path(candidates: &[&str]) -> Option<TokenStream> {
    candidates
        .iter()
        .find_map(|name| crate_name(name).ok())
        .map(found_crate_path)
}

fn found_crate_path(found: FoundCrate) -> TokenStream {
    match found {
        FoundCrate::Itself => quote!(crate),
        FoundCrate::Name(name) => extern_path(&name),
    }
}

fn extern_path(name: &str) -> TokenStream {
    let ident = Ident::new(name, Span::call_site());
    quote!(::#ident)
}

/// Whether `package` is the crate currently being compiled.
fn is_self(package: &str) -> bool {
    std::env::var("CARGO_PKG_NAME").as_deref() == Ok(package)
}
