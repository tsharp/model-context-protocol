//! `#[mcp_server]` and `#[mcp_internal]` macro implementations.
//!
//! Processes server structs and impl blocks to generate MCP protocol methods.

use darling::FromMeta;
use proc_macro2::TokenStream;
use quote::quote;
use syn::{parse2, Attribute, FnArg, ImplItem, ItemImpl, ItemStruct, Lit, Meta};

use crate::schema::is_option_type;
use crate::tool::{has_param_attr, parse_param_attrs, strip_param_attrs, CollectedTool, ParamInfo};

/// Arguments for `#[mcp_server]` on structs.
#[derive(Debug, Default, FromMeta)]
pub struct ServerArgs {
    /// Server name (required on struct).
    #[darling(default)]
    pub name: Option<String>,

    /// Server version (optional).
    #[darling(default)]
    pub version: Option<String>,

    /// Server description (optional).
    #[darling(default)]
    pub description: Option<String>,
}

/// Implementation of `#[mcp_server]`.
pub fn mcp_server_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Try parsing as impl block first
    if let Ok(impl_block) = parse2::<ItemImpl>(item.clone()) {
        return process_impl_block(impl_block);
    }

    // Try parsing as struct
    if let Ok(struct_item) = parse2::<ItemStruct>(item.clone()) {
        let args = parse_server_args(attr);
        return process_struct(struct_item, args);
    }

    syn::Error::new_spanned(
        item,
        "mcp_server can only be applied to structs or impl blocks",
    )
    .to_compile_error()
}

fn parse_server_args(attr: TokenStream) -> ServerArgs {
    if attr.is_empty() {
        return ServerArgs::default();
    }

    let meta: Meta = match parse2(quote! { mcp_server(#attr) }) {
        Ok(m) => m,
        Err(_) => return ServerArgs::default(),
    };

    ServerArgs::from_meta(&meta).unwrap_or_default()
}

fn process_struct(struct_item: ItemStruct, args: ServerArgs) -> TokenStream {
    let struct_name = &struct_item.ident;
    let server_name = args
        .name
        .unwrap_or_else(|| struct_name.to_string().to_lowercase());

    // Version is now optional - returns Option<&'static str>
    let version_impl = match args.version {
        Some(v) => quote! { Some(#v) },
        None => quote! { None },
    };

    // Description is optional - returns Option<&'static str>
    let description_impl = match args.description {
        Some(d) => quote! { Some(#d) },
        None => quote! { None },
    };

    quote! {
        #struct_item

        impl #struct_name {
            /// Returns the MCP server name.
            pub fn name() -> &'static str {
                #server_name
            }

            /// Returns the MCP server version (optional).
            pub fn version() -> Option<&'static str> {
                #version_impl
            }

            /// Returns the MCP server description (optional).
            pub fn description() -> Option<&'static str> {
                #description_impl
            }
        }
    }
}

fn process_impl_block(impl_block: ItemImpl) -> TokenStream {
    let struct_ty = &impl_block.self_ty;

    // Collect tools from methods with #[mcp_tool_meta] or #[mcp_tool]
    let tools = match collect_tools(&impl_block) {
        Ok(t) => t,
        Err(e) => return e.to_compile_error(),
    };

    // Generate tool definitions for McpTool
    let tool_defs: Vec<TokenStream> = tools.iter().map(|t| t.generate_mcp_tool_def()).collect();

    // Generate call_tool match arms
    let call_arms: Vec<TokenStream> = tools.iter().map(|t| t.generate_call_arm()).collect();

    // Strip mcp_tool_meta and mcp_tool attributes from methods, and #[param] from parameters
    let cleaned_items: Vec<TokenStream> = impl_block
        .items
        .iter()
        .map(|item| {
            if let ImplItem::Fn(method) = item {
                let mut cleaned = method.clone();
                // Strip #[mcp_tool] and #[mcp_tool_meta] from method
                cleaned
                    .attrs
                    .retain(|a| !is_mcp_tool_meta(a) && !is_mcp_tool(a));
                // Strip #[param] from parameters
                for input in &mut cleaned.sig.inputs {
                    if let FnArg::Typed(pat_type) = input {
                        strip_param_attrs(&mut pat_type.attrs);
                    }
                }
                quote! { #cleaned }
            } else {
                quote! { #item }
            }
        })
        .collect();

    quote! {
        impl #struct_ty {
            #(#cleaned_items)*
        }

        impl mcp::MacroServer for #struct_ty {
            /// List all MCP tools provided by this server.
            fn list_tools(&self) -> Vec<mcp::McpToolDefinition> {
                vec![
                    #(#tool_defs),*
                ]
            }

            /// Execute a tool call by name.
            fn call_tool(&self, name: &str, args: serde_json::Value) -> mcp::ToolCallResult {
                let args = args.as_object().cloned().unwrap_or_default();

                let result: Result<mcp::ToolCallResult, String> = (|| {
                    Ok(match name {
                        #(#call_arms)*
                        _ => return Err(format!("Unknown tool: {}", name)),
                    })
                })();

                match result {
                    Ok(r) => r,
                    Err(e) => Err(e),
                }
            }
        }
    }
}

/// Collect tools from an impl block by looking for `#[mcp_tool_meta]` attributes.
fn collect_tools(impl_block: &ItemImpl) -> Result<Vec<CollectedTool>, syn::Error> {
    let mut tools = Vec::new();

    for item in &impl_block.items {
        if let ImplItem::Fn(method) = item {
            if let Some(tool) = extract_tool_from_method(method)? {
                tools.push(tool);
            }
        }
    }

    Ok(tools)
}

/// Extract tool metadata from a method's attributes.
fn extract_tool_from_method(method: &syn::ImplItemFn) -> Result<Option<CollectedTool>, syn::Error> {
    // Look for #[mcp_tool_meta(...)] or #[mcp_tool(...)]
    for attr in &method.attrs {
        if is_mcp_tool_meta(attr) || is_mcp_tool(attr) {
            let (name, description) = parse_tool_meta_attr(attr, &method.sig.ident.to_string());
            let params = extract_params_from_sig(&method.sig)?;

            return Ok(Some(CollectedTool {
                name,
                description,
                params,
                method_ident: method.sig.ident.clone(),
            }));
        }
    }

    Ok(None)
}

fn is_mcp_tool_meta(attr: &Attribute) -> bool {
    attr.path().is_ident("mcp_tool_meta")
}

fn is_mcp_tool(attr: &Attribute) -> bool {
    attr.path().is_ident("mcp_tool")
}

fn parse_tool_meta_attr(attr: &Attribute, default_name: &str) -> (String, String) {
    let mut name = default_name.to_string();
    let mut description = String::new();

    // Try shorthand first: #[mcp_tool("description")]
    if let Ok(Lit::Str(s)) = attr.parse_args::<Lit>() {
        return (name, s.value());
    }

    // Try to parse the full form: #[mcp_tool(description = "...", name = "...")]
    if let Ok(nested) = attr
        .parse_args_with(syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated)
    {
        for meta in nested {
            if let syn::Meta::NameValue(nv) = meta {
                if nv.path.is_ident("name") {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(s), ..
                    }) = &nv.value
                    {
                        name = s.value();
                    }
                } else if nv.path.is_ident("description") {
                    if let syn::Expr::Lit(syn::ExprLit {
                        lit: Lit::Str(s), ..
                    }) = &nv.value
                    {
                        description = s.value();
                    }
                }
            }
        }
    }

    (name, description)
}

fn extract_params_from_sig(sig: &syn::Signature) -> Result<Vec<ParamInfo>, syn::Error> {
    let mut params = Vec::new();

    for input in &sig.inputs {
        if let syn::FnArg::Typed(pat_type) = input {
            if let syn::Pat::Ident(pat_ident) = pat_type.pat.as_ref() {
                let name = pat_ident.ident.to_string();
                if name == "self" {
                    continue;
                }

                // Require all parameters to be marked with #[param]
                if !has_param_attr(&pat_type.attrs) {
                    return Err(syn::Error::new_spanned(
                        pat_ident,
                        format!(
                            "Parameter `{}` must be marked with #[param(\"description\")]. \
                            All tool parameters require explicit #[param] attributes.",
                            name
                        ),
                    ));
                }

                // Parse the #[param] attribute
                let param_args = parse_param_attrs(&pat_type.attrs).unwrap_or_default();

                // Priority: explicit attr description > doc comment
                let description = param_args.description.or_else(|| {
                    pat_type.attrs.iter().find_map(|attr| {
                        if attr.path().is_ident("doc") {
                            if let Meta::NameValue(meta) = &attr.meta {
                                if let syn::Expr::Lit(expr_lit) = &meta.value {
                                    if let Lit::Str(lit_str) = &expr_lit.lit {
                                        return Some(lit_str.value().trim().to_string());
                                    }
                                }
                            }
                        }
                        None
                    })
                });

                // Use custom name if provided, otherwise use argument name
                let param_name = param_args.name.unwrap_or(name);

                // Use explicit required if provided, otherwise infer from Option<T>
                let required = param_args
                    .required
                    .unwrap_or_else(|| !is_option_type(&pat_type.ty));

                params.push(ParamInfo {
                    name: param_name,
                    ty: (*pat_type.ty).clone(),
                    description,
                    required,
                });
            }
        }
    }

    Ok(params)
}
