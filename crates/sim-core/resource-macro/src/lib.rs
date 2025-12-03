// SPDX-License-Identifier: MIT

//! Proc-macro crate for SIMH-RS resource definitions.
//!
//! # `#[derive(SimResources)]`
//!
//! Generates `impl sim_core::env::SimResourceProvider for StructName { ... }` from field-level
//! `#[resource(...)]` attributes.
//!
//! ## Field attribute syntax
//!
//! ```text
//! #[resource(
//!     name    = <expr>,          // required: &str or const &str
//!     bits    = <expr>,          // required: bit-width as u32
//!     len     = <expr>,          // optional: array length; default 1 (scalar)
//!     fmt     = <path>,          // optional: fn(u64)->String pointer, wrapped in Some() automatically
//!     get     = <expr>,          // optional: custom getter (must return Result<u64, SimError>)
//!     set     = <stmt(s)>,       // optional: custom setter (may use `val: u64` and `index: usize`)
//!     shift   = <expr>,          // optional: bit-shift for packed fields; auto-generates get/set
//!     readonly                   // optional: bare flag; write_resource returns ReadOnlyResource
//! )]
//! ```
//!
//! Multiple `#[resource(...)]` attributes on a single field are supported for packed registers (e.g.
//! L packed in the accumulator word).  Resource IDs are assigned in declaration order across all
//! fields.
//!
//! ## Defaults when `get` / `set` are omitted
//!
//! | Case           | get                                | set                                   |
//! |----------------|------------------------------------|---------------------------------------|
//! | scalar         | `Ok(self.field as u64)`            | `self.field = val as _`               |
//! | scalar bool    | `Ok(self.field as u64)`            | `self.field = val != 0`               |
//! | array (`len`)  | `self.field.get(index).map(...)` | `*self.field.get_mut(index)? = val as _` |
//! | shift          | masked-shift expression            | mask-and-insert expression            |
//!
//! Shift auto-set uses the field's declared type for the mask arithmetic, so it works correctly
//! for `u16`, `u32`, etc.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TS2, TokenTree};
use quote::quote;
use std::collections::HashMap;
use syn::{parse_macro_input, Data, DeriveInput, Fields};

// ─── public derive entry-point ────────────────────────────────────────────────

#[proc_macro_derive(SimResources, attributes(resource))]
pub fn derive_sim_resources(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    match impl_sim_resources(&input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

// ─── internal data ────────────────────────────────────────────────────────────

struct ResourceDef {
    resource_id: u32,
    /// Tokens for the name expression (evaluates to &str or String)
    name_ts: TS2,
    /// Tokens for the description expression (evaluates to &str or String)
    descr_ts: TS2,
    /// Tokens for bits expression (evaluates to u32-compatible)
    bits_ts: TS2,
    /// Tokens for len expression (evaluates to usize-compatible)
    len_ts: TS2,
    /// Tokens for formatter — evaluates to `Option<fn(u64) -> String>`
    fmt_ts: TS2,
    /// Complete getter expression — must evaluate to Result<u64, SimError>
    get_ts: TS2,
    /// Statement(s) to execute on write — may use `val: u64` and `index: usize`
    set_ts: TS2,
    /// Whether to emit a ReadOnlyResource check before the set statements
    readonly: bool,
}

// ─── core impl ────────────────────────────────────────────────────────────────

fn impl_sim_resources(input: &DeriveInput) -> syn::Result<TS2> {
    let struct_name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let named_fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => &f.named,
            _ => {
                return Err(syn::Error::new_spanned(
                    input,
                    "SimResources only supports structs with named fields",
                ))
            }
        },
        _ => {
            return Err(syn::Error::new_spanned(
                input,
                "SimResources only supports structs",
            ))
        }
    };

    let mut resources: Vec<ResourceDef> = Vec::new();
    let mut next_id: u32 = 0;

    for field in named_fields {
        let field_ident = field.ident.as_ref().unwrap();
        let field_ty = &field.ty;

        for attr in &field.attrs {
            if !attr.path().is_ident("resource") {
                continue;
            }

            let args = parse_kv_attr(attr)?;

            // ── required parameters ──
            let name_ts = args
                .get("name")
                .cloned()
                .ok_or_else(|| syn::Error::new_spanned(attr, "#[resource] requires `name`"))?;

            let descr_ts = args.get("descr").cloned().unwrap_or_else(|| name_ts.clone()); // Default description is the name

            let bits_ts = args
                .get("bits")
                .cloned()
                .ok_or_else(|| syn::Error::new_spanned(attr, "#[resource] requires `bits`"))?;

            // ── optional parameters ──
            let is_array = args.contains_key("len");
            let len_ts = args.get("len").cloned().unwrap_or_else(|| quote! { 1usize });

            let fmt_ts = match args.get("fmt") {
                Some(f) => quote! { Some(#f) },
                None => quote! { None },
            };

            let readonly = args.contains_key("readonly");
            let shift_ts = args.get("shift").cloned();

            // ── getter ──
            let get_ts = if let Some(custom) = args.get("get") {
                custom.clone()
            } else if let Some(shift) = &shift_ts {
                quote! {
                    Ok((
                        ((self.#field_ident as u64) >> (#shift as u32)) & ((1u64 << (#bits_ts as u32)) - 1)
                    ) as u64)
                }
            } else if is_array {
                quote! {
                    self.#field_ident
                        .get(index)
                        .map(|&v| v as u64)
                        .ok_or(::sim_core::env::SimError::AddressBounds)
                }
            } else {
                quote! { Ok(self.#field_ident as u64) }
            };

            // ── setter ──
            let set_ts = if let Some(custom) = args.get("set") {
                custom.clone()
            } else if let Some(shift) = &shift_ts {
                // Auto-generate mask-and-insert using the field's declared type.
                // Works for u8, u16, u32, u64 (and usize).
                quote! {
                    {
                        let __bits = #bits_ts as u32;
                        let __shift = #shift as u32;
                        let __mask = if __bits >= 64 { u64::MAX } else { (1u64 << __bits) - 1 };
                        let __field_mask = __mask << __shift;
                        let __val = self.#field_ident as u64;
                        self.#field_ident = ((__val & !__field_mask) | ((val & __mask) << __shift)) as #field_ty;
                    }
                }
            } else if is_array {
                quote! {
                    *self.#field_ident
                        .get_mut(index)
                        .ok_or(::sim_core::env::SimError::AddressBounds)? = val as _
                }
            } else if is_bool_type(field_ty) {
                quote! { self.#field_ident = val != 0 }
            } else {
                quote! { self.#field_ident = val as _ }
            };

            resources.push(ResourceDef {
                resource_id: next_id,
                name_ts,
                descr_ts,
                bits_ts,
                len_ts,
                fmt_ts,
                get_ts,
                set_ts,
                readonly,
            });
            next_id += 1;
        }
    }

    // ── code-gen ─────────────────────────────────────────────────────────────

    let metadata_entries: Vec<TS2> = resources
        .iter()
        .map(|r| {
            let id = r.resource_id;
            let name = &r.name_ts;
            let descr = &r.descr_ts;
            let bits = &r.bits_ts;
            let len = &r.len_ts;
            let fmt = &r.fmt_ts;
            let ro = r.readonly;
            quote! {
                ::sim_core::env::ResourceMetadata {
                    name:        { #name }.to_ascii_uppercase(),
                    description: { #descr }.to_string(),
                    resource_id: #id,
                    word_size:   (#bits) as u32,
                    length:      (#len) as usize,
                    formatter:   #fmt,
                    read_only:   #ro,
                }
            }
        })
        .collect();

    let read_arms: Vec<TS2> = resources
        .iter()
        .map(|r| {
            let id = r.resource_id;
            let get = &r.get_ts;
            quote! { #id => { #get } }
        })
        .collect();

    let write_arms: Vec<TS2> = resources
        .iter()
        .map(|r| {
            let id = r.resource_id;
            let name = &r.name_ts;
            let bits = &r.bits_ts;
            let set = &r.set_ts;
            let ro = r.readonly;
            quote! {
                #id => {
                    if #ro {
                        return Err(::sim_core::env::SimError::ReadOnlyResource(
                            { #name }.to_string()
                        ));
                    }
                    if (#bits as u32) < 64u32 {
                        let __max = (1u64 << (#bits as u32)) - 1;
                        if val > __max {
                            return Err(::sim_core::env::SimError::ValueOutOfRange {
                                value: val,
                                bits: #bits as usize,
                                resource: { #name }.to_string(),
                            });
                        }
                    }
                    #set;
                    Ok(())
                }
            }
        })
        .collect();

    Ok(quote! {
        impl #impl_generics ::sim_core::env::SimResourceProvider
            for #struct_name #ty_generics #where_clause
        {
            fn get_metadata(&self) -> Vec<::sim_core::env::ResourceMetadata> {
                vec![ #( #metadata_entries ),* ]
            }

            fn read_resource(
                &self,
                res_id: u32,
                index: usize,
            ) -> Result<u64, ::sim_core::env::SimError> {
                match res_id {
                    #( #read_arms, )*
                    _ => Err(::sim_core::env::SimError::AddressBounds),
                }
            }

            fn write_resource(
                &mut self,
                res_id: u32,
                index: usize,
                val: u64,
            ) -> Result<(), ::sim_core::env::SimError> {
                match res_id {
                    #( #write_arms, )*
                    _ => Err(::sim_core::env::SimError::AddressBounds),
                }
            }
        }
    })
}

// ─── attribute parser ─────────────────────────────────────────────────────────

/// Parse `#[resource(key = tokens, key = tokens, bare_flag)]` into a map.
///
/// Values are collected as raw `TokenStream` segments — everything between `=` and the next
/// top-level `,` (or end of list).  Since `proc_macro2::TokenTree::Group` is always a balanced
/// bracket group, scanning for `,` at the top level naturally skips over `(...)`, `[...]`, `{...}`
/// without needing explicit depth tracking.
fn parse_kv_attr(attr: &syn::Attribute) -> syn::Result<HashMap<String, TS2>> {
    let list = match &attr.meta {
        syn::Meta::List(ml) => ml,
        _ => {
            return Err(syn::Error::new_spanned(
                attr,
                "expected #[resource(key = value, ...)]",
            ))
        }
    };

    let mut map: HashMap<String, TS2> = HashMap::new();
    let mut iter = list.tokens.clone().into_iter().peekable();

    while let Some(tt) = iter.next() {
        // Expect an identifier as the key.
        let key = match tt {
            TokenTree::Ident(i) => i.to_string(),
            other => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!("expected identifier, got {:?}", other),
                ))
            }
        };

        match iter.peek() {
            // key = value
            Some(TokenTree::Punct(p)) if p.as_char() == '=' => {
                iter.next(); // consume `=`
                let mut value = TS2::new();
                loop {
                    match iter.peek() {
                        None => break,
                        Some(TokenTree::Punct(p)) if p.as_char() == ',' => {
                            iter.next(); // consume `,`
                            break;
                        }
                        _ => {
                            value.extend(std::iter::once(iter.next().unwrap()));
                        }
                    }
                }
                map.insert(key, value);
            }
            // bare flag followed by comma
            Some(TokenTree::Punct(p)) if p.as_char() == ',' => {
                iter.next();
                map.insert(key, quote! { true });
            }
            // bare flag at end
            None => {
                map.insert(key, quote! { true });
            }
            // unexpected
            Some(other) => {
                return Err(syn::Error::new(
                    Span::call_site(),
                    format!("expected `=` or `,` after `{}`, got {:?}", key, other),
                ))
            }
        }
    }

    Ok(map)
}

/// `true` when the type is exactly `bool` (handles the special set default).
fn is_bool_type(ty: &syn::Type) -> bool {
    if let syn::Type::Path(tp) = ty {
        tp.qself.is_none() && tp.path.is_ident("bool")
    } else {
        false
    }
}
