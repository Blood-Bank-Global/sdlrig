use core::panic;
use proc_macro;
use proc_macro2;
use quote::{format_ident, quote, ToTokens, TokenStreamExt};
use std::{
    collections::{HashMap, HashSet},
    str::FromStr,
};
use syn;

#[proc_macro_derive(Adjustable, attributes(adjustable))]
pub fn adjustable_macro_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let ast: syn::DeriveInput = syn::parse(input).unwrap();
    let struct_ident = ast.ident;
    let struct_generics = ast.generics;
    let (impl_generics, ty_generics, where_clause) = struct_generics.split_for_impl();

    let adjustable_struct = match ast.data {
        syn::Data::Struct(data_struct) => data_struct,
        _ => panic!("Only supports struct types"),
    };

    let fields = match adjustable_struct.fields {
        syn::Fields::Named(fields_named) => fields_named,
        _ => panic!("Only supports named fields"),
    };

    let mut q = quote! {};

    let mut adjuster_map = HashMap::new();
    let mut field_data = HashSet::new();

    for field in fields.named.iter() {
        let ident = field.ident.as_ref().unwrap();
        for attr in &field.attrs {
            let args = attr.meta.require_list().unwrap().clone();
            let mut toks = args.tokens.clone().into_iter();
            let mut params = HashMap::new();
            loop {
                let helper_name = if let Some(helper_name) = toks.next() {
                    syn::parse2::<proc_macro2::Ident>(helper_name.into()).unwrap()
                } else {
                    break; // no more toks
                };

                toks.next(); // disregard =

                let mut value_tokens = proc_macro2::TokenStream::new();
                loop {
                    match &toks.next() {
                        None => break,
                        Some(proc_macro2::TokenTree::Punct(punct)) => {
                            if punct.as_char() == ',' {
                                break;
                            }
                            value_tokens.extend(punct.to_token_stream());
                        }
                        Some(next) => value_tokens.extend(next.to_token_stream()),
                    }
                }

                params.insert(helper_name.to_string(), value_tokens);
            }

            if attr.path().require_ident().unwrap() == "adjustable" {
                let ty = if let Some(ty) = params.remove("ty") {
                    ty
                } else {
                    field.ty.to_token_stream()
                };

                let adjustment_ident = if let Some(name) = params.remove("name") {
                    format_ident!("adjust_{}", name.to_string())
                } else {
                    format_ident!("adjust_{}", ident)
                };

                let scale_ident = if let Some(name) = params.remove("name") {
                    format_ident!("scale_{}", name.to_string())
                } else {
                    format_ident!("scale_{}", ident)
                };

                let min_ident = if let Some(name) = params.remove("name") {
                    format_ident!("min_{}", name.to_string())
                } else {
                    format_ident!("min_{}", ident)
                };

                let max_ident = if let Some(name) = params.remove("name") {
                    format_ident!("max_{}", name.to_string())
                } else {
                    format_ident!("max_{}", ident)
                };

                let getter = if let Some(getter) = params.remove("getter") {
                    syn::parse2::<proc_macro2::Ident>(getter.clone()).unwrap()
                } else {
                    q.extend(quote! {
                        impl #impl_generics #struct_ident #ty_generics #where_clause {
                            pub fn #ident(&self) -> #ty {
                                self.#ident
                            }
                        }
                    });
                    ident.clone()
                };

                let setter = if let Some(setter) = params.remove("setter") {
                    syn::parse2::<proc_macro2::Ident>(setter.clone()).unwrap()
                } else {
                    let setter = format_ident!("set_{}", ident);
                    q.extend(quote! {
                        impl #impl_generics #struct_ident #ty_generics #where_clause {
                            pub fn #setter(&mut self, v: #ty) {
                                self.#ident = v;
                            }
                        }
                    });
                    setter
                };

                let commander = if let Some(command_simple) = params.remove("command_simple") {
                    let group = match syn::parse2::<proc_macro2::Group>(command_simple) {
                        Ok(group) => group,
                        Err(e) => panic!("{}", e),
                    };

                    let mut mix = proc_macro2::TokenStream::new();
                    let mut name = proc_macro2::TokenStream::new();
                    let mut ptype = proc_macro2::TokenStream::new();
                    let mut iter = group.stream().into_iter();
                    while let Some(token_tree) = iter.next() {
                        if let proc_macro2::TokenTree::Punct(punct) = &token_tree {
                            if punct.as_char() == ',' {
                                break;
                            }
                        }
                        mix.append(token_tree);
                    }

                    while let Some(token_tree) = iter.next() {
                        if let proc_macro2::TokenTree::Punct(punct) = &token_tree {
                            if punct.as_char() == ',' {
                                break;
                            }
                        }
                        name.append(token_tree);
                    }

                    while let Some(token_tree) = iter.next() {
                        if let proc_macro2::TokenTree::Punct(punct) = &token_tree {
                            if punct.as_char() == ',' {
                                break;
                            }
                        }
                        ptype.append(token_tree);
                    }
                    let prim = match ptype.to_string().as_str() {
                        "Float" => quote! { f32 },
                        "Integer" => quote! { i32 },
                        "Unsigned" => quote! { u32 },
                        _ => panic!("Unknown type {}", ptype),
                    };

                    let command_sender_ident = format_ident!("command_{}_spec", ident);
                    q.extend( quote! {
                        impl #impl_generics #struct_ident #ty_generics #where_clause {
                            pub fn #command_sender_ident(&self) -> Vec<sdlrig::renderspec::RenderSpec> {
                                vec![sdlrig::renderspec::SendCmd::builder()
                                    .mix(#mix)
                                    .name(#name)
                                    .value(sdlrig::renderspec::SendValue::#ptype (self.#getter() as #prim ))
                                    .build()
                                    .into()
                                ]
                            }
                        }
                    });
                    Some(command_sender_ident)
                } else if let Some(command_fn) = params.remove("command_fn") {
                    let command_fn = format_ident!("{}", command_fn.to_string());
                    Some(command_fn)
                } else {
                    None
                };

                let do_not_record = match params.remove("do_not_record") {
                    Some(val) => {
                        if val.to_string().to_ascii_lowercase() == "true" {
                            true
                        } else {
                            false
                        }
                    }
                    _ => false,
                };

                let tween = match params.remove("tween") {
                    Some(val) => {
                        if val.to_string().to_ascii_lowercase() == "false"
                            || val.to_string().to_ascii_lowercase() == "0"
                            || val.to_string().to_ascii_lowercase() == "0.0"
                        {
                            false
                        } else {
                            true
                        }
                    }
                    _ => false,
                };

                field_data.insert((
                    ident.clone(),
                    format_ident!("{}", ident.to_string().to_uppercase()),
                    field.ty.clone(),
                    commander,
                    do_not_record,
                    tween,
                ));

                {
                    let knob = params.remove("k");
                    let index = params.remove("idx");
                    if let (Some(knob), Some(index)) = (&knob, &index) {
                        if let Some(_) = adjuster_map.insert(
                            (
                                knob.to_string().clone(),
                                usize::from_str(&index.to_string()).unwrap(),
                            ),
                            adjustment_ident.clone(),
                        ) {
                            panic!(
                                "Multiple instances of k={} idx={}",
                                knob.to_string(),
                                index.to_string()
                            );
                        }
                    } else if (knob.is_none() && index.is_some())
                        || (knob.is_some() && index.is_none())
                    {
                        panic!(
                            "Must specify knob  (k) and index (idx) {:?} or neither",
                            args
                        );
                    }
                }

                let field_kind = match params.remove("kind") {
                    Some(t) => t.to_string(),
                    _ => String::from("step"),
                };

                if field_kind == "step" {
                    let min = if let Some(min) = params.remove("min") {
                        quote! { ((#min) as f64) }
                    } else {
                        quote! { #ty::MIN }
                    };
                    let max = if let Some(max) = params.remove("max") {
                        quote! { ((#max) as f64) }
                    } else {
                        quote! { #ty::MAX }
                    };
                    let step = if let Some(step) = params.remove("step") {
                        quote! { (({ #step }) as f64)}
                    } else {
                        quote! { 1.0f64 }
                    };

                    let clamp_ident = format_ident!("clamp_{}", setter);
                    q.extend(quote! {
                        impl #impl_generics #struct_ident #ty_generics #where_clause {
                            pub fn #adjustment_ident(&mut self, inc: f64) {
                                let v = self.#getter() as f64 + inc * ((#step) as f64);
                                self.#setter(v.clamp(#min, #max) as #ty);
                            }

                            pub fn #scale_ident(&mut self, p: f64) {
                                let v = ((#max - #min) * p) + #min;
                                self.#setter(v.clamp(#min, #max) as #ty);
                            }

                            pub fn #clamp_ident(&mut self, v: f64) {
                                self.#setter(v.clamp(#min as f64, #max as f64));
                            }

                            pub fn #max_ident(&self) -> #ty {
                                #max
                            }

                            pub fn #min_ident(&self) -> #ty{
                                #min
                            }
                        }
                    });
                } else if field_kind == "custom" {
                    // do nothing -- assume user has implemented something of the following form:
                    // adjust_#ident(&mut self, inc: f64)
                } else if field_kind == "toggle" {
                    let toggle_ident = format_ident!("toggle_{}", ident);
                    q.extend(quote! {
                        impl #impl_generics #struct_ident #ty_generics #where_clause {
                           pub fn #toggle_ident(&mut self) {
                                if self.#ident as u8 == 0 {
                                    self.#setter(true as u8);
                                } else {
                                    self.#setter(false as u8);
                                }
                            }

                           pub  fn #adjustment_ident(&mut self, _: f64) {
                                self.#toggle_ident();
                            }
                        }
                    });
                } else if field_kind == "assign" {
                    let from = params.remove("from");
                    let assign_ident = format_ident!("assign_to_{}", ident);
                    q.extend(quote! {
                        impl #impl_generics #struct_ident #ty_generics #where_clause {

                            pub fn #assign_ident(&mut self) {
                                self.#setter(#from as #ty);
                            }

                            pub fn #adjustment_ident(&mut self, _: f64) {
                                self.#assign_ident();
                            }
                        }
                    });
                } else {
                    panic!("Unknown field type {:?}", field);
                }
            } else {
                continue;
            }

            assert!(
                params.is_empty(),
                "Unknown parameters: {} {:?}",
                ident.to_string(),
                params
            );
        }
    }

    let all_ident = format_ident!("ALL_{}_UPDATERS", struct_ident.to_string().to_uppercase());
    let field_enum_ident = format_ident!("{}AllFieldsEnum", struct_ident);
    let field_change_ident = format_ident!("{}AllFieldsChange", struct_ident);

    let mut knobs = vec![];
    let mut indexes = vec![];
    let mut adjusters = vec![];
    for ((knob, index), adjuster) in adjuster_map {
        knobs.push(format_ident!("{}", knob));
        indexes.push(proc_macro2::Literal::usize_suffixed(index));
        adjusters.push(adjuster);
    }

    q.extend(quote! {
        impl #impl_generics #struct_ident #ty_generics #where_clause {
            fn adjust(&mut self, kn: sdlrig::gfxinfo::Knob, idx: usize, inc: f64) {
                match (kn, idx) {
                    #((sdlrig::gfxinfo::Knob::#knobs, #indexes) => {
                        self.#adjusters(inc);
                    })*
                    _ => (),
                }
            }
        }
    });

    let _count = proc_macro2::Literal::usize_suffixed(field_data.len());
    let mut field_idents = vec![];
    let mut field_enums = vec![];
    let mut field_tys = vec![];
    let mut to_new_value = vec![];
    let mut from_new_value = vec![];
    let mut field_enum_with_commanders = vec![];
    let mut field_enum_do_not_record = vec![];
    let mut commanders = vec![];
    let mut tweens = vec![];
    let mut tween_field_idents = vec![];
    let mut tween_tys = vec![];

    for (field_ident, field_enum, field_ty, commander, do_not_record, tween) in &field_data {
        field_idents.push(field_ident);
        field_enums.push(field_enum);
        field_tys.push(field_ty);
        let fragement_from_new_value;
        let fragment_to_new_value;
        if field_ty.clone().to_token_stream().to_string() == "()" {
            fragment_to_new_value = quote! { 0.0 };
            fragement_from_new_value = quote! { () };
        } else if field_ty.clone().to_token_stream().to_string() == "f64" {
            fragment_to_new_value = quote! { other.#field_ident };
            fragement_from_new_value = quote! { self.#field_ident = diff.new_value };
        } else {
            fragment_to_new_value = quote! { other.#field_ident as f64 };
            fragement_from_new_value = quote! { self.#field_ident = diff.new_value as #field_ty };
        }
        to_new_value.push(fragment_to_new_value.clone());
        from_new_value.push(fragement_from_new_value.clone());

        if let Some(commander) = commander {
            field_enum_with_commanders.push(field_enum);
            commanders.push(commander);
        }
        if *do_not_record {
            field_enum_do_not_record.push(field_enum);
        }
        if *tween {
            tweens.push(field_enum);
            tween_field_idents.push(field_ident);
            tween_tys.push(field_ty);
        }
    }
    let count = proc_macro2::Literal::usize_suffixed(commanders.len());
    let ty_generics_turbo = ty_generics.as_turbofish();
    q.extend(quote! {
        impl #impl_generics #struct_ident #ty_generics #where_clause {
            pub const #all_ident : [fn(&#struct_ident #ty_generics ) -> Vec<sdlrig::renderspec::RenderSpec>; #count] = [
                #(#struct_ident #ty_generics_turbo ::#commanders),*
            ];
        }
    });

    let diff_code = quote! {
        #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        pub enum #field_enum_ident {
            #(#field_enums),*
        }

        #[derive(Debug, Copy, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
        pub struct #field_change_ident {
            pub field: #field_enum_ident,
            pub new_value: f64,
        }

        impl #field_enum_ident {
            pub fn can_tween(&self) -> bool {
                match self {
                    #(
                        #field_enum_ident::#tweens => true,
                    )*
                    _ => false,
                }
            }
        }

        impl #impl_generics #struct_ident #ty_generics #where_clause {
            pub fn should_record(f: &#field_enum_ident) -> bool {
                match f {
                    #(#field_enum_ident :: #field_enum_do_not_record => false,)*
                    _ => true,
                }
            }
        }

        impl #impl_generics #struct_ident #ty_generics #where_clause {
            pub fn diff(&self, other: &#struct_ident #ty_generics) -> Vec<#field_change_ident> {
                let mut diffs = vec![];
                #(
                    if self.#field_idents != other.#field_idents {
                        diffs.push(#field_change_ident {
                            field: #field_enum_ident::#field_enums,
                            new_value: #to_new_value,
                        });
                    }
                )*
                diffs
            }

            pub fn apply_diff(&mut self, diffs: &[#field_change_ident]) {
                for diff in diffs {
                    match diff.field {
                        #(
                            #field_enum_ident::#field_enums => #from_new_value
                        ),*
                    }
                }
            }

            pub fn get_commands(&self, fields: &[#field_enum_ident]) -> Vec<sdlrig::renderspec::RenderSpec> {
                let mut commands = vec![];
                for field in fields {
                    match field {
                        #(#field_enum_ident::#field_enum_with_commanders => { commands.extend(self.#commanders()); })*
                        _ => (),
                    }
                }
                commands
            }

            pub fn get_field_value(&self, field: #field_enum_ident) -> Option<f64> {
                let other = self; // groan
                match field {
                    #(
                        #field_enum_ident::#field_enums => Some(#to_new_value),

                    )*
                    _ => None,
                }
            }

            pub fn tween_diff(&self, start: f64, end: #field_change_ident, proportion: f64) -> Option<#field_change_ident> {
                match end.field {
                    #(
                        #field_enum_ident::#tweens => {
                            let delta = (end.new_value - start) * proportion;
                            let new_value = start + delta;
                            Some(
                                #field_change_ident {
                                    field: #field_enum_ident::#tweens,
                                    new_value,
                                }
                            )
                        },
                    )*
                    _ => None
                }
            }
        }
    };
    q.extend(diff_code);
    q.into()
}
