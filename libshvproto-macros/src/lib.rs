use convert_case::{Case, Casing};
use syn::punctuated::Punctuated;
use syn::{Meta, Token};

use core::panic;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;

fn is_option(ty: &syn::Type) -> bool {
    let syn::Type::Path(typepath) = ty else {
        return false
    };
    if !(typepath.qself.is_none() && typepath.path.segments.len() == 1) {
        return false
    }
    let segment = &typepath.path.segments[0];

    if segment.ident != "Option" {
        return false;
    }
    let syn::PathArguments::AngleBracketed(ref data) = segment.arguments else {
        return false;
    };
    if data.args.len() != 1 {
        return false;
    }

    matches!(data.args[0], syn::GenericArgument::Type(_))
}

fn get_type(ty: &syn::Type) -> Option<String> {
    match ty {
        syn::Type::Path(typepath) if typepath.qself.is_none() => {
            typepath.path.segments
            .last()
            .map(|x| x.ident.to_string())
        }
        syn::Type::Tuple(tuple) if tuple.elems.is_empty() => Some("()".to_owned()),
        _ => None,
    }
}

fn get_field_name(field: &syn::Field) -> String {
    field
        .attrs.first()
        .and_then(|attr| attr.meta.require_name_value().ok())
        .filter(|meta_name_value| meta_name_value.path.is_ident("field_name"))
        .map(|meta_name_value| if let syn::Expr::Lit(expr) = &meta_name_value.value { expr } else { panic!("Expected a string literal for 'field_name'") })
        .map(|literal| if let syn::Lit::Str(expr) = &literal.lit { expr.value() } else { panic!("Expected a string literal for 'field_name'") })
        .unwrap_or_else(|| field.ident.as_ref().unwrap().to_string().to_case(Case::Camel))
}

fn field_to_initializers(
    field_name: &str,
    identifier: &syn::Ident,
    is_option: bool,
    from_value: Option<TokenStream2>,
    context: &str,
) -> (TokenStream2, TokenStream2)
{
    let struct_initializer;
    let rpcvalue_insert;
    let identifier_at_value = if let Some(value) = from_value {
        quote! { #value.#identifier }
    } else {
        quote! { #identifier }
    };
    if is_option {
        struct_initializer = quote!{
            #identifier: match get_key(#field_name).ok() {
                Some(x) => Some(x.try_into().map_err(|e| format!("{}Cannot parse `{}` field: {e}", #context, #field_name))?),
                None => None,
            },
        };
        rpcvalue_insert = quote!{
            if let Some(val) = #identifier_at_value {
                map.insert(#field_name.into(), val.into());
            }
        };
    } else {
        struct_initializer = quote!{
            #identifier: get_key(#field_name)
                .and_then(|x| x.try_into().map_err(|e| format!("Cannot parse `{}` field: {e}", #field_name)))
                .map_err(|e| format!("{}{e}", #context))?,
        };
        rpcvalue_insert = quote!{
            map.insert(#field_name.into(), #identifier_at_value.into());
        };
    }
    (struct_initializer, rpcvalue_insert)
}

#[derive(Default)]
struct StructAttributes {
    tag: Option<String>,
}

fn parse_struct_attributes(attrs: &Vec<syn::Attribute>) -> syn::Result<StructAttributes> {
    let mut res = StructAttributes::default();
    for attr in attrs {
        if attr.path().is_ident("rpcvalue") {
            let nested = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
            for meta in &nested {
                match meta {
                    // #[rpcvalue(tag = "type")]
                    Meta::NameValue(name_value) if name_value.path.is_ident("tag") => {
                        let syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Str(tag_lit), .. }) = &name_value.value else {
                            return Err(syn::Error::new_spanned(meta, "tag value is not a string literal"));
                        };
                        res.tag = Some(tag_lit.value());
                    }
                    _ => return Err(syn::Error::new_spanned(meta, "unrecognized attributes")),
                }
            }
        }
    }
    Ok(res)
}

#[proc_macro_derive(TryFromRpcValue, attributes(field_name,rpcvalue))]
pub fn derive_from_rpcvalue(item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::DeriveInput);
    let struct_identifier = &input.ident;
    let struct_generics_without_bounds_vec = input.generics.params.clone().into_iter().map(|generic_param|
        if let syn::GenericParam::Type(mut type_param) = generic_param {
            type_param.bounds.clear();
            syn::GenericParam::Type(type_param)
        } else {
            generic_param
        }).collect::<Vec<_>>();
    let struct_generics_without_bounds = quote!(<#(#struct_generics_without_bounds_vec),*>);
    let struct_generics_with_bounds = quote!{<#(#struct_generics_without_bounds_vec: Into<shvproto::RpcValue> + for<'a> TryFrom<&'a shvproto::RpcValue, Error = String>),*>};

    match &input.data {
        syn::Data::Struct(syn::DataStruct { fields, .. }) => {
            let mut struct_initializers = quote!{};
            let mut expected_keys = vec![];
            let mut rpcvalue_inserts = quote!{};
            for field in fields {
                let field_name = get_field_name(field);
                let (struct_initializer, rpcvalue_insert) = field_to_initializers(
                    &field_name,
                    field.ident.as_ref().expect("Missing field identifier"),
                    is_option(&field.ty),
                    Some(quote! { value }),
                    "",
                );
                struct_initializers.extend(struct_initializer);
                rpcvalue_inserts.extend(rpcvalue_insert);
                expected_keys.push(quote!{#field_name});
            }

            quote!{
                impl #struct_generics_with_bounds TryFrom<shvproto::RpcValue> for #struct_identifier #struct_generics_without_bounds {
                    type Error = String;
                    fn try_from(value: shvproto::RpcValue) -> Result<Self, <Self as TryFrom<shvproto::RpcValue>>::Error> {
                        if let shvproto::Value::Map(value) = value.value() {
                            value.try_into()
                        } else {
                            Err("Value is not a map".into())
                        }
                    }
                }

                impl #struct_generics_with_bounds TryFrom<&shvproto::RpcValue> for #struct_identifier #struct_generics_without_bounds {
                    type Error = String;
                    fn try_from(value: &shvproto::RpcValue) -> Result<Self, <Self as TryFrom<&shvproto::RpcValue>>::Error> {
                        if let shvproto::Value::Map(value) = value.value() {
                            value.as_ref().try_into()
                        } else {
                            Err("Value is not a map".into())
                        }
                    }
                }

                impl #struct_generics_with_bounds  TryFrom<&Box<shvproto::Map>> for #struct_identifier #struct_generics_without_bounds  {
                    type Error = String;
                    fn try_from(value: &Box<shvproto::Map>) -> Result<Self, <Self as TryFrom<&Box<shvproto::Map>>>::Error> {
                        value.as_ref().try_into()
                    }
                }

                impl #struct_generics_with_bounds TryFrom<&shvproto::Map> for #struct_identifier #struct_generics_without_bounds  {
                    type Error = String;
                    fn try_from(value: &shvproto::Map) -> Result<Self, <Self as TryFrom<&shvproto::Map>>::Error> {
                        let get_key = |key_name| value.get(key_name).ok_or_else(|| "Missing `".to_string() + key_name + "` key");
                        let unexpected_keys = value
                            .keys()
                            .map(String::as_str)
                            .filter(|k| ![#(#expected_keys),*].contains(k))
                            .collect::<Vec<_>>();
                        if unexpected_keys.is_empty() {
                            Ok(Self {
                                #struct_initializers
                            })
                        } else {
                            Err(["Cannot parse Map, unexpected keys: ", &unexpected_keys.join(", ")].concat())
                        }
                    }
                }

                impl #struct_generics_with_bounds TryFrom<shvproto::Map> for #struct_identifier #struct_generics_without_bounds {
                    type Error = String;
                    fn try_from(value: shvproto::Map) -> Result<Self, <Self as TryFrom<shvproto::Map>>::Error> {
                        Self::try_from(&value)
                    }
                }

                impl #struct_generics_with_bounds From<#struct_identifier #struct_generics_without_bounds> for shvproto::RpcValue {
                    fn from(value: #struct_identifier #struct_generics_without_bounds) -> Self {
                        let mut map = shvproto::rpcvalue::Map::new();
                        #rpcvalue_inserts
                        map.into()
                    }
                }
            }
        },
        syn::Data::Enum(syn::DataEnum { variants, .. }) => {
            let mut match_arms_de = vec![];
            let mut match_arms_ser  = quote!{};
            let mut allowed_types = vec![];
            let mut custom_type_matchers = vec![];
            let mut match_arms_tags = vec![];
            let struct_attributes = parse_struct_attributes(&input.attrs).unwrap();
            let mut map_has_been_matched_as_map: Option<(proc_macro2::TokenStream, proc_macro2::TokenStream)> = None;
            let mut map_has_been_matched_as_struct: Option<Vec<(proc_macro2::TokenStream, proc_macro2::TokenStream)>> = None;
            for variant in variants {
                let variant_ident = &variant.ident;
                let mut add_type_matcher = |match_arms: &mut Vec<proc_macro2::TokenStream>, rpcvalue_variant_type, allowed_variant_type, block| {
                    allowed_types.push(quote!{stringify!(#allowed_variant_type)});

                    match_arms.push(quote!{
                        shvproto::Value::#rpcvalue_variant_type => Ok(#struct_identifier::#variant_ident #block)
                    });
                };
                match &variant.fields {
                    syn::Fields::Unnamed(variant_types) => {
                        if variant_types.unnamed.len() != 1 {
                            panic!("Only single element variant tuples are supported for TryFromRpcValue");
                        }
                        match_arms_ser.extend(quote!{
                            #struct_identifier::#variant_ident(val) => shvproto::RpcValue::from(val),
                        });

                        let source_variant_type = &variant_types.unnamed.first().expect("No tuple elements").ty;
                        let deref_code = quote!((*x));
                        let unbox_code = quote!((x.as_ref().clone()));

                        if let Some(type_identifier) = get_type(source_variant_type) {
                            match type_identifier.as_ref() {
                                "()" => add_type_matcher(&mut match_arms_de, quote!{Null}, quote!{Null}, quote!{(())}),
                                "i64" => add_type_matcher(&mut match_arms_de, quote!{Int(x)}, quote!{Int}, deref_code.clone()),
                                "u64" => add_type_matcher(&mut match_arms_de, quote!{UInt(x)}, quote!{UInt}, deref_code.clone()),
                                "f64" => add_type_matcher(&mut match_arms_de, quote!{Double(x)}, quote!{Double}, deref_code.clone()),
                                "bool" => add_type_matcher(&mut match_arms_de, quote!{Bool(x)}, quote!{Bool}, deref_code.clone()),
                                "DateTime" => add_type_matcher(&mut match_arms_de, quote!{DateTime(x)}, quote!{DateTime}, deref_code.clone()),
                                "Decimal" => add_type_matcher(&mut match_arms_de, quote!{Decimal(x)}, quote!{Decimal}, deref_code.clone()),
                                "String" => add_type_matcher(&mut match_arms_de, quote!{String(x)}, quote!{String}, unbox_code.clone()),
                                "Blob" => add_type_matcher(&mut match_arms_de, quote!{Blob(x)}, quote!{Blob}, unbox_code.clone()),
                                "List" => add_type_matcher(&mut match_arms_de, quote!{List(x)}, quote!{List}, unbox_code.clone()),
                                "Map" => {
                                    if let Some((matched_variant_ident, matched_variant_type)) = map_has_been_matched_as_map {
                                        panic!("Can't match enum variant {}(Map), because a Map will already be matched as {}({})", quote!{#variant_ident}, quote!{#matched_variant_ident}, quote!{#matched_variant_type});
                                    }
                                    if let Some(matched_variants) = map_has_been_matched_as_struct {
                                        let matched_variants = matched_variants
                                            .iter()
                                            .map(|(ident, ty)| quote!{ #ident(#ty) })
                                            .collect::<Vec<proc_macro2::TokenStream>>();
                                        panic!("Can't match enum variant {}(Map), because a Map will already be matched as one of: {}",
                                            quote!{#variant_ident},
                                            quote!{#(#matched_variants),*}
                                        );
                                    }
                                    add_type_matcher(&mut match_arms_de,  quote!{Map(x)}, quote!{Map}, unbox_code.clone());
                                    map_has_been_matched_as_map = Some((quote!(#variant_ident), quote!{#source_variant_type}));
                                },
                                "IMap" => add_type_matcher(&mut match_arms_de,  quote!{IMap(x)}, quote!{IMap}, unbox_code.clone()),
                                _ => {
                                    if let Some((matched_variant_ident, matched_variant_type)) = map_has_been_matched_as_map {
                                        panic!("Can't match enum variant {}({}) as a Map, because a Map will already be matched as {}({})",
                                            quote!{#variant_ident},
                                            quote!{#source_variant_type},
                                            quote!{#matched_variant_ident},
                                            quote!{#matched_variant_type}
                                        );
                                    }
                                    custom_type_matchers.push(quote!{
                                        if let Ok(val) = #source_variant_type::try_from(x.as_ref().clone()) {
                                            return Ok(#struct_identifier::#variant_ident(val));
                                        }
                                    });
                                    map_has_been_matched_as_struct
                                        .get_or_insert(vec![])
                                        .push((quote!(#variant_ident), quote!(#source_variant_type)));
                                }

                            }
                        }
                    },
                    syn::Fields::Unit => {
                        let variant_ident_name = variant_ident.to_string().to_case(Case::Camel);
                        match_arms_ser.extend(quote!{
                            #struct_identifier::#variant_ident => shvproto::RpcValue::from(#variant_ident_name),
                        });
                        add_type_matcher(&mut match_arms_de, quote!{String(s) if s.as_str() == #variant_ident_name}, quote!{#variant_ident}, quote!());
                    },
                    syn::Fields::Named(variant_fields) => {
                        if let Some((matched_variant_ident, matched_variant_type)) = map_has_been_matched_as_map {
                            panic!("Can't match enum variant {}(...) as a Map, because a Map will already be matched as {}({})",
                                quote!{#variant_ident},
                                quote!{#matched_variant_ident},
                                quote!{#matched_variant_type}
                            );
                        }

                        let mut struct_initializers = quote! {};
                        let mut rpcvalue_inserts = quote! {};
                        let mut field_idents = vec![];
                        let variant_ident_name = variant_ident.to_string().to_case(Case::Camel);

                        for field in &variant_fields.named {
                            let field_ident = field.ident.as_ref().expect("Missing field identifier");
                            let (struct_initializer, rpcvalue_insert) = field_to_initializers(
                                get_field_name(field).as_str(),
                                field_ident,
                                is_option(&field.ty),
                                None,
                                &format!("Cannot deserialize into `{}` enum variant: ", variant_ident_name.as_str()),
                            );
                            struct_initializers.extend(struct_initializer);
                            rpcvalue_inserts.extend(rpcvalue_insert);
                            field_idents.push(field_ident);
                        }

                        if let Some(tag_key) = &struct_attributes.tag {
                            rpcvalue_inserts.extend(quote! {
                                map.insert(#tag_key.into(), #variant_ident_name.into());
                            });
                        } else {
                            rpcvalue_inserts = quote! {
                                map.insert(#variant_ident_name.into(), {
                                    let mut map = shvproto::rpcvalue::Map::new();
                                    #rpcvalue_inserts
                                    map.into()
                                });
                            };
                        }

                        match_arms_ser.extend(quote!{
                            #struct_identifier::#variant_ident{ #(#field_idents),* } => {
                                let mut map = shvproto::rpcvalue::Map::new();
                                #rpcvalue_inserts
                                map.into()
                            }
                        });

                        match_arms_tags.push(quote! {
                            #variant_ident_name => Ok(#struct_identifier::#variant_ident {
                                #struct_initializers
                            }),
                        });

                        map_has_been_matched_as_struct
                            .get_or_insert(vec![])
                            .push((quote!(#variant_ident), quote!(#(#field_idents)*)));
                    },
                }
            }

            if !match_arms_tags.is_empty() {
                if let Some(tag_key) = &struct_attributes.tag {
                    custom_type_matchers.push(quote! {
                        let tag: String = x.get(#tag_key)
                            .ok_or_else(|| "Missing `".to_string() + #tag_key + "` key")
                            .and_then(|val| val
                                .try_into()
                                .map_err(|e: String| "Cannot parse `".to_string() + #tag_key + "` field: " + &e)
                            )
                            .map_err(|e| "Cannot get tag: ".to_string() + &e)?;
                        });
                    } else {
                        custom_type_matchers.push(quote! {
                            let (tag, val) = x.iter().nth(0).ok_or_else(|| "Cannot get tag from an empty Map")?;
                            let x: shvproto::rpcvalue::Map = val.try_into()?;
                        });
                }

                custom_type_matchers.push(quote! {
                    let get_key = |key_name| x
                        .get(key_name)
                        .ok_or_else(|| "Missing `".to_string() + key_name + "` key");

                    match tag.as_str() {
                        #(#match_arms_tags)*
                        _ => Err("Couldn't deserialize into `".to_owned() + stringify!(#struct_identifier) + "` enum from a Map. Unknown tag `" + &tag + "`"),
                    }
                });
            }

            if !custom_type_matchers.is_empty() {
                allowed_types.push(quote!{stringify!(Map(x))});
                if match_arms_tags.is_empty() {
                    // The match in match_arms_tags is the last expression returned from
                    // custom_type_matchers. If match_arms_tags is empty, just return Err.
                    custom_type_matchers.push(quote!{
                        Err("Couldn't deserialize into `".to_owned() + stringify!(#struct_identifier) + "` enum from a Map.")
                    });
                }
                match_arms_de.push(quote!{
                    shvproto::Value::Map(x) => {
                        #(#custom_type_matchers)*
                    }
                });
            }

            quote!{
                impl TryFrom<shvproto::RpcValue> for #struct_identifier {
                    type Error = String;
                    fn try_from(value: shvproto::RpcValue) -> Result<Self, <Self as TryFrom<shvproto::RpcValue>>::Error> {
                        match value.value() {
                            #(#match_arms_de),*,
                            _ => Err("Couldn't deserialize into `".to_owned() + stringify!(#struct_identifier) + "` enum, allowed types: " + [#(#allowed_types),*].join("|").as_ref() + ", got: " + value.type_name())
                        }
                    }
                }

                impl TryFrom<&shvproto::RpcValue> for #struct_identifier {
                    type Error = String;
                    fn try_from(value: &shvproto::RpcValue) -> Result<Self, <Self as TryFrom<&shvproto::RpcValue>>::Error> {
                        value.clone().try_into()
                    }
                }

                impl From<#struct_identifier> for shvproto::RpcValue {
                    fn from(value: #struct_identifier) -> Self {
                        match value {
                            #match_arms_ser
                        }
                    }
                }
            }
        }
        _ => panic!("This macro can only be used on a struct or an enum.")
    }.into()
}
