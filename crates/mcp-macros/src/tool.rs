//! `#[mcp_tool]` macro implementation.
//!
//! Processes tool function/method attributes and generates McpTool implementations.

use darling::FromMeta;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse2, FnArg, ImplItemFn, ItemFn, Lit, Meta, Pat, Type};

use crate::schema::{is_option_type, type_to_schema};

/// Convert a Rust type to a JSON type string.
pub fn rust_type_to_json_type(ty: &Type) -> &'static str {
    match ty {
        Type::Path(type_path) => {
            if let Some(segment) = type_path.path.segments.last() {
                let ident = segment.ident.to_string();
                match ident.as_str() {
                    "String" | "str" => "string",
                    "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32"
                    | "u64" | "u128" | "usize" => "integer",
                    "f32" | "f64" => "number",
                    "bool" => "boolean",
                    "Vec" => "array",
                    "Option" => {
                        // Extract inner type
                        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                                return rust_type_to_json_type(inner_ty);
                            }
                        }
                        "object"
                    }
                    _ => "object",
                }
            } else {
                "object"
            }
        }
        Type::Reference(type_ref) => rust_type_to_json_type(&type_ref.elem),
        _ => "object",
    }
}

/// Arguments for the `#[mcp_tool]` attribute.
#[derive(Debug, FromMeta)]
pub struct ToolArgs {
    /// Tool description shown to the LLM.
    pub description: String,

    /// Optional custom tool name (defaults to function name).
    #[darling(default)]
    pub name: Option<String>,

    /// Optional group/category for organizing tools.
    #[darling(default)]
    pub group: Option<String>,
}

/// Parsed parameter information.
#[derive(Debug, Clone)]
pub struct ParamInfo {
    pub name: String,
    pub ty: syn::Type,
    pub description: Option<String>,
    pub required: bool,
}

/// Implementation of `#[mcp_tool]`.
pub fn mcp_tool_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    // Parse attributes
    let attr_args = match parse_tool_args(attr) {
        Ok(args) => args,
        Err(e) => return e.to_compile_error(),
    };

    // Try parsing as standalone function first, then impl method
    // (ItemFn is more specific than ImplItemFn for top-level functions)
    if let Ok(func) = parse2::<ItemFn>(item.clone()) {
        process_standalone_function(func, attr_args)
    } else if let Ok(method) = parse2::<ImplItemFn>(item.clone()) {
        process_impl_method(method, attr_args)
    } else {
        syn::Error::new_spanned(item, "mcp_tool can only be applied to functions or methods")
            .to_compile_error()
    }
}

fn parse_tool_args(attr: TokenStream) -> Result<ToolArgs, syn::Error> {
    let meta = if attr.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "mcp_tool requires a description: #[mcp_tool(description = \"...\")]",
        ));
    } else {
        let parsed: Meta = parse2(quote! { mcp_tool(#attr) })?;
        parsed
    };

    ToolArgs::from_meta(&meta).map_err(|e| syn::Error::new(proc_macro2::Span::call_site(), e))
}

/// Process a method inside an impl block - generates metadata for #[mcp_server] to collect
fn process_impl_method(method: ImplItemFn, args: ToolArgs) -> TokenStream {
    let params = extract_params(&method.sig.inputs.iter().collect::<Vec<_>>());
    let tool_name = args.name.unwrap_or_else(|| method.sig.ident.to_string());

    // Store metadata as an attribute for collection by #[mcp_server]
    let description = &args.description;
    let param_tokens = generate_param_metadata(&params);

    quote! {
        #[doc(hidden)]
        #[allow(dead_code)]
        const _: () = {
            // Tool metadata stored for #[mcp_server] to collect
        };

        // Preserve the original method with metadata attribute
        #[doc = #description]
        #[mcp_tool_meta(name = #tool_name, description = #description, params = [#param_tokens])]
        #method
    }
}

/// Process a standalone function - generates a struct that implements McpTool
fn process_standalone_function(func: ItemFn, args: ToolArgs) -> TokenStream {
    let func_name = &func.sig.ident;
    let tool_name = args.name.unwrap_or_else(|| func_name.to_string());
    let description = &args.description;

    // Generate group code - either Some("...".to_string()) or None
    let group_code = match &args.group {
        Some(g) => quote! { Some(#g.to_string()) },
        None => quote! { None },
    };

    // Generate a PascalCase struct name from the function name
    let struct_name = format_ident!("{}Tool", to_pascal_case(&func_name.to_string()));

    let params = extract_params(&func.sig.inputs.iter().collect::<Vec<_>>());
    let is_async = func.sig.asyncness.is_some();

    // Generate the JSON schema properties
    let properties = generate_json_properties(&params);
    let required: Vec<&str> = params
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name.as_str())
        .collect();

    // Generate parameter extraction code
    let param_extractions: Vec<TokenStream> = params
        .iter()
        .map(|p| {
            let param_name = &p.name;
            let param_ident = syn::Ident::new(&p.name, proc_macro2::Span::call_site());
            let ty = &p.ty;

            if is_option_type(ty) {
                quote! {
                    let #param_ident: #ty = __args
                        .get(#param_name)
                        .and_then(|v| serde_json::from_value(v.clone()).ok());
                }
            } else {
                quote! {
                    let #param_ident: #ty = {
                        let __raw = __args
                            .get(#param_name)
                            .ok_or_else(|| format!("Missing required parameter: {}", #param_name))?
                            .clone();
                        serde_json::from_value(__raw)
                            .map_err(|e| format!("Invalid parameter '{}': {}", #param_name, e))?
                    };
                }
            }
        })
        .collect();

    let param_names: Vec<syn::Ident> = params
        .iter()
        .map(|p| syn::Ident::new(&p.name, proc_macro2::Span::call_site()))
        .collect();

    // Generate the call expression
    let call_expr = if is_async {
        quote! { #func_name(#(#param_names),*).await }
    } else {
        quote! { #func_name(#(#param_names),*) }
    };

    // Check return type for Result
    let result_handling = generate_result_handling(&func.sig.output, call_expr);

    // Generate the group for inventory registration
    let inventory_group = match &args.group {
        Some(g) => quote! { Some(#g) },
        None => quote! { None },
    };

    quote! {
        // Keep the original function
        #[doc = #description]
        #func

        /// Auto-generated tool wrapper for the `#func_name` function.
        #[derive(Clone, Copy, Default)]
        pub struct #struct_name;

        impl mcp::McpTool for #struct_name {
            fn definition(&self) -> mcp::McpToolDef {
                mcp::McpToolDef {
                    name: #tool_name.to_string(),
                    description: Some(#description.to_string()),
                    group: #group_code,
                    input_schema: serde_json::json!({
                        "type": "object",
                        "properties": { #properties },
                        "required": [#(#required),*]
                    }),
                }
            }

            fn call<'a>(&'a self, __args: serde_json::Value) -> mcp::BoxFuture<'a, mcp::ToolCallResult> {
                Box::pin(async move {
                    let __args = __args.as_object().cloned().unwrap_or_default();
                    #(#param_extractions)*
                    #result_handling
                })
            }
        }

        // Register with inventory for auto-discovery
        mcp::inventory::submit! {
            mcp::ToolEntry::new(
                || std::sync::Arc::new(#struct_name) as mcp::DynTool,
                #inventory_group
            )
        }
    }
}

/// Convert snake_case to PascalCase
fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => first.to_uppercase().chain(chars).collect(),
            }
        })
        .collect()
}

/// Generate result handling code based on return type
fn generate_result_handling(output: &syn::ReturnType, call_expr: TokenStream) -> TokenStream {
    match output {
        syn::ReturnType::Default => {
            // No return value - just call and return success
            quote! {
                #call_expr;
                Ok(vec![mcp::ToolContent::text("ok")])
            }
        }
        syn::ReturnType::Type(_, ty) => {
            // Check if it's a Result type
            if is_result_type(ty) {
                quote! {
                    match #call_expr {
                        Ok(value) => {
                            let text = serde_json::to_string(&value)
                                .unwrap_or_else(|_| format!("{:?}", value));
                            Ok(vec![mcp::ToolContent::text(text)])
                        }
                        Err(e) => Err(format!("{}", e)),
                    }
                }
            } else {
                // Direct return - serialize result
                quote! {
                    let __result = #call_expr;
                    let text = serde_json::to_string(&__result)
                        .unwrap_or_else(|_| format!("{:?}", __result));
                    Ok(vec![mcp::ToolContent::text(text)])
                }
            }
        }
    }
}

/// Check if a type is a Result
fn is_result_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Result";
        }
    }
    false
}

/// Extract parameter information from function arguments.
fn extract_params(inputs: &[&FnArg]) -> Vec<ParamInfo> {
    let mut params = Vec::new();

    for input in inputs {
        if let FnArg::Typed(pat_type) = input {
            // Skip self parameters
            if let Pat::Ident(pat_ident) = pat_type.pat.as_ref() {
                let name = pat_ident.ident.to_string();
                if name == "self" {
                    continue;
                }

                // Extract doc comment from attributes
                let description = extract_doc_comment(&pat_type.attrs);

                // Check if optional
                let required = !is_option_type(&pat_type.ty);

                params.push(ParamInfo {
                    name,
                    ty: (*pat_type.ty).clone(),
                    description,
                    required,
                });
            }
        }
    }

    params
}

/// Extract doc comment from attributes.
fn extract_doc_comment(attrs: &[syn::Attribute]) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident("doc") {
            if let Meta::NameValue(meta) = &attr.meta {
                if let syn::Expr::Lit(expr_lit) = &meta.value {
                    if let Lit::Str(lit_str) = &expr_lit.lit {
                        return Some(lit_str.value().trim().to_string());
                    }
                }
            }
        }
    }
    None
}

/// Generate token stream for parameter metadata.
fn generate_param_metadata(params: &[ParamInfo]) -> TokenStream {
    let param_tokens: Vec<TokenStream> = params
        .iter()
        .map(|p| {
            let name = &p.name;
            let ty = &p.ty;
            let desc = p.description.as_deref().unwrap_or("");
            let required = p.required;
            let schema = type_to_schema(ty);

            quote! {
                McpParamMeta {
                    name: #name,
                    description: #desc,
                    required: #required,
                    schema: #schema,
                }
            }
        })
        .collect();

    quote! { #(#param_tokens),* }
}

/// Generate JSON properties for tool schema.
fn generate_json_properties(params: &[ParamInfo]) -> TokenStream {
    let props: Vec<TokenStream> = params
        .iter()
        .map(|p| {
            let name = &p.name;
            let ty_str = rust_type_to_json_type(&p.ty);
            let desc = p.description.as_deref().unwrap_or("");

            if desc.is_empty() {
                quote! { #name: { "type": #ty_str } }
            } else {
                quote! { #name: { "type": #ty_str, "description": #desc } }
            }
        })
        .collect();

    quote! { #(#props),* }
}

/// Represents collected tool metadata for code generation (used by mcp_server macro).
#[derive(Debug, Clone)]
pub struct CollectedTool {
    pub name: String,
    pub description: String,
    pub params: Vec<ParamInfo>,
    pub method_ident: syn::Ident,
}

impl CollectedTool {
    /// Generate the `McpToolDef` struct initialization.
    pub fn generate_mcp_tool_def(&self) -> TokenStream {
        let name = &self.name;
        let description = &self.description;
        let properties = self.generate_json_properties();
        let required: Vec<&str> = self
            .params
            .iter()
            .filter(|p| p.required)
            .map(|p| p.name.as_str())
            .collect();

        quote! {
            mcp::McpToolDef {
                name: #name.to_string(),
                description: Some(#description.to_string()),
                group: None,
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": { #properties },
                    "required": [#(#required),*]
                }),
            }
        }
    }

    /// Generate JSON properties for tool schema.
    fn generate_json_properties(&self) -> TokenStream {
        let props: Vec<TokenStream> = self
            .params
            .iter()
            .map(|p| {
                let name = &p.name;
                let ty_str = rust_type_to_json_type(&p.ty);
                let desc = p.description.as_deref().unwrap_or("");

                if desc.is_empty() {
                    quote! { #name: { "type": #ty_str } }
                } else {
                    quote! { #name: { "type": #ty_str, "description": #desc } }
                }
            })
            .collect();

        quote! { #(#props),* }
    }

    /// Generate the match arm for calling this tool.
    pub fn generate_call_arm(&self) -> TokenStream {
        let name = &self.name;
        let method = &self.method_ident;

        let param_extractions: Vec<TokenStream> = self
            .params
            .iter()
            .map(|p| {
                let param_name = &p.name;
                let param_ident = syn::Ident::new(&p.name, proc_macro2::Span::call_site());
                let ty = &p.ty;

                if is_option_type(ty) {
                    quote! {
                        let #param_ident: #ty = args
                            .get(#param_name)
                            .and_then(|v| serde_json::from_value(v.clone()).ok());
                    }
                } else {
                    quote! {
                        let #param_ident: #ty = {
                            let __raw = args
                                .get(#param_name)
                                .ok_or_else(|| format!("Missing required parameter: {}", #param_name))?
                                .clone();
                            serde_json::from_value(__raw)
                                .map_err(|e| format!("Invalid parameter '{}': {}", #param_name, e))?
                        };
                    }
                }
            })
            .collect();

        let param_names: Vec<syn::Ident> = self
            .params
            .iter()
            .map(|p| syn::Ident::new(&p.name, proc_macro2::Span::call_site()))
            .collect();

        quote! {
            #name => {
                #(#param_extractions)*
                let result = self.#method(#(#param_names),*);
                match serde_json::to_string(&result) {
                    Ok(json) => Ok(vec![mcp::ToolContent::text(json)]),
                    Err(e) => Err(format!("Serialization error: {}", e)),
                }
            }
        }
    }
}
