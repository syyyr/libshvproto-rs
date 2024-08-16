use convert_case::{Case, Casing};

use core::panic;

use proc_macro::TokenStream;
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

#[proc_macro_derive(TryFromRpcValue, attributes(field_name))]
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
                let identifier = field.ident.as_ref().unwrap();
                let field_name = field
                    .attrs.first()
                    .and_then(|attr| attr.meta.require_name_value().ok())
                    .filter(|meta_name_value| meta_name_value.path.is_ident("field_name"))
                    .map(|meta_name_value| if let syn::Expr::Lit(expr) = &meta_name_value.value { expr } else { panic!("Expected a string literal for 'field_name'") })
                    .map(|literal| if let syn::Lit::Str(expr) = &literal.lit { expr.value() } else { panic!("Expected a string literal for 'field_name'") })
                    .unwrap_or_else(|| identifier.to_string().to_case(Case::Camel));

                if is_option(&field.ty) {
                    struct_initializers.extend(quote!{
                        #identifier: match get_key(#field_name).ok() {
                            Some(x) => Some(x.try_into().map_err(|e| format!("Cannot parse `{}` field: {e}", #field_name))?),
                            None => None,
                        },
                    });
                    rpcvalue_inserts.extend(quote!{
                        if let Some(val) = value.#identifier {
                            map.insert(#field_name.into(), val.into());
                        }
                    });
                } else {
                    struct_initializers.extend(quote!{
                        #identifier: get_key(#field_name)
                            .and_then(|x| x.try_into().map_err(|e| format!("Cannot parse `{}` field: {e}", #field_name)))?,
                    });
                    rpcvalue_inserts.extend(quote!{
                        map.insert(#field_name.into(), value.#identifier.into());
                    });
                }
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
                        let get_key = |key_name| value.get(key_name).ok_or_else(|| "Missing ".to_string() + key_name + " key");
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
                                        panic!("Can't match enum variant {}(Map), because a Map will already be matched as one of: {}", quote!{#variant_ident}, quote!{#(#matched_variants),*});
                                    }
                                    add_type_matcher(&mut match_arms_de,  quote!{Map(x)}, quote!{Map}, unbox_code.clone());
                                    map_has_been_matched_as_map = Some((quote!(#variant_ident), quote!{#source_variant_type}));
                                },
                                "IMap" => add_type_matcher(&mut match_arms_de,  quote!{IMap(x)}, quote!{IMap}, unbox_code.clone()),
                                _ => {
                                    if let Some((matched_variant_ident, matched_variant_type)) = map_has_been_matched_as_map {
                                        panic!("Can't match enum variant {}({}) as a Map, because a Map will already be matched as {}({})", quote!{#variant_ident}, quote!{#source_variant_type}, quote!{#matched_variant_ident}, quote!{#matched_variant_type});
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
                        match_arms_ser.extend(quote!{
                            #struct_identifier::#variant_ident => shvproto::RpcValue::from(stringify!(#variant_ident)),
                        });
                        add_type_matcher(&mut match_arms_de, quote!{String(s) if s.as_str() == stringify!(#variant_ident)}, quote!{#variant_ident}, quote!());
                    },
                    syn::Fields::Named(_) => ()
                }
            }

            if !custom_type_matchers.is_empty() {
                allowed_types.push(quote!{stringify!(Map(x))});
                custom_type_matchers.push(quote!{
                    Err("Couldn't deserialize into '".to_owned() + stringify!(#struct_identifier) + "' enum from a Map.")
                });
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
                            _ => Err("Couldn't deserialize into '".to_owned() + stringify!(#struct_identifier) + "' enum, allowed types: " + [#(#allowed_types),*].join("|").as_ref() + ", got: " + value.type_name())
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
