//! JSON Schema generation from Rust types.
//!
//! Converts Rust types to JSON Schema definitions for MCP tool parameters.

use proc_macro2::TokenStream;
use quote::quote;
use syn::Type;

/// Generate JSON schema properties for a Rust type.
///
/// Maps common Rust types to their JSON Schema equivalents:
/// - `String`, `&str` → `{"type": "string"}`
/// - `i32`, `i64`, `u32`, `u64`, etc. → `{"type": "integer"}`
/// - `f32`, `f64` → `{"type": "number"}`
/// - `bool` → `{"type": "boolean"}`
/// - `Vec<T>` → `{"type": "array", "items": ...}`
/// - `Option<T>` → schema for T (not required)
/// - `Value` → `{}` (any type)
/// - Other → `{"type": "object"}`
pub fn type_to_schema(ty: &Type) -> TokenStream {
    match ty {
        Type::Path(type_path) => {
            let segments = &type_path.path.segments;
            if let Some(segment) = segments.last() {
                let ident = segment.ident.to_string();
                match ident.as_str() {
                    "String" | "str" => {
                        quote! { serde_json::json!({"type": "string"}) }
                    }
                    "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32"
                    | "u64" | "u128" | "usize" => {
                        quote! { serde_json::json!({"type": "integer"}) }
                    }
                    "f32" | "f64" => {
                        quote! { serde_json::json!({"type": "number"}) }
                    }
                    "bool" => {
                        quote! { serde_json::json!({"type": "boolean"}) }
                    }
                    "Value" => {
                        // serde_json::Value - any JSON type
                        quote! { serde_json::json!({}) }
                    }
                    "Vec" => {
                        // Extract inner type for array items
                        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                                let inner_schema = type_to_schema(inner_ty);
                                return quote! {
                                    serde_json::json!({
                                        "type": "array",
                                        "items": #inner_schema
                                    })
                                };
                            }
                        }
                        quote! { serde_json::json!({"type": "array"}) }
                    }
                    "Option" => {
                        // Extract inner type - Option makes it not required, not nullable
                        if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                            if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                                return type_to_schema(inner_ty);
                            }
                        }
                        quote! { serde_json::json!({}) }
                    }
                    "HashMap" | "BTreeMap" => {
                        quote! { serde_json::json!({"type": "object"}) }
                    }
                    // Handle known enum types from the protocol (2025-11-25)
                    "TaskStatus" => {
                        quote! {
                            serde_json::json!({
                                "type": "string",
                                "enum": ["working", "input_required", "completed", "failed", "cancelled"]
                            })
                        }
                    }
                    "TaskSupport" => {
                        quote! {
                            serde_json::json!({
                                "type": "string",
                                "enum": ["forbidden", "optional", "required"]
                            })
                        }
                    }
                    "ElicitAction" => {
                        quote! {
                            serde_json::json!({
                                "type": "string",
                                "enum": ["accept", "decline", "cancel"]
                            })
                        }
                    }
                    "Role" => {
                        quote! {
                            serde_json::json!({
                                "type": "string",
                                "enum": ["user", "assistant"]
                            })
                        }
                    }
                    "LoggingLevel" => {
                        quote! {
                            serde_json::json!({
                                "type": "string",
                                "enum": ["debug", "info", "notice", "warning", "error", "critical", "alert", "emergency"]
                            })
                        }
                    }
                    "ToolChoiceMode" => {
                        quote! {
                            serde_json::json!({
                                "type": "string",
                                "enum": ["auto", "required", "none"]
                            })
                        }
                    }
                    "IconTheme" => {
                        quote! {
                            serde_json::json!({
                                "type": "string",
                                "enum": ["light", "dark"]
                            })
                        }
                    }
                    "StringSchemaFormat" => {
                        quote! {
                            serde_json::json!({
                                "type": "string",
                                "enum": ["email", "uri", "date", "date-time"]
                            })
                        }
                    }
                    _ => {
                        // Default to object for custom types
                        quote! { serde_json::json!({"type": "object"}) }
                    }
                }
            } else {
                quote! { serde_json::json!({}) }
            }
        }
        Type::Reference(type_ref) => {
            // Handle &str, &T
            type_to_schema(&type_ref.elem)
        }
        _ => {
            quote! { serde_json::json!({}) }
        }
    }
}

/// Generate a oneOf schema for discriminated unions.
/// This is used for types like ElicitRequestParams, ToolContent, etc.
#[allow(dead_code)]
pub fn generate_one_of_schema(variants: &[&str]) -> TokenStream {
    let variant_schemas: Vec<TokenStream> = variants
        .iter()
        .map(|v| quote! { serde_json::json!({"$ref": #v}) })
        .collect();

    quote! {
        serde_json::json!({
            "oneOf": [#(#variant_schemas),*]
        })
    }
}

/// Generate an enum schema with string values.
#[allow(dead_code)]
pub fn generate_enum_schema(values: &[&str]) -> TokenStream {
    quote! {
        serde_json::json!({
            "type": "string",
            "enum": [#(#values),*]
        })
    }
}

/// Check if a type is `Option<T>`.
pub fn is_option_type(ty: &Type) -> bool {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            return segment.ident == "Option";
        }
    }
    false
}

/// Extract the inner type from `Option<T>`.
#[allow(dead_code)]
pub fn extract_option_inner(ty: &Type) -> Option<&Type> {
    if let Type::Path(type_path) = ty {
        if let Some(segment) = type_path.path.segments.last() {
            if segment.ident == "Option" {
                if let syn::PathArguments::AngleBracketed(args) = &segment.arguments {
                    if let Some(syn::GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Some(inner_ty);
                    }
                }
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn test_string_type() {
        let ty: Type = parse_quote!(String);
        let schema = type_to_schema(&ty);
        assert!(!schema.is_empty());
    }

    #[test]
    fn test_option_detection() {
        let opt_ty: Type = parse_quote!(Option<String>);
        let string_ty: Type = parse_quote!(String);

        assert!(is_option_type(&opt_ty));
        assert!(!is_option_type(&string_ty));
    }

    #[test]
    fn test_enum_type() {
        let ty: Type = parse_quote!(TaskStatus);
        let schema = type_to_schema(&ty);
        assert!(!schema.is_empty());
    }

    #[test]
    fn test_role_enum() {
        let ty: Type = parse_quote!(Role);
        let schema = type_to_schema(&ty);
        assert!(!schema.is_empty());
    }
}
