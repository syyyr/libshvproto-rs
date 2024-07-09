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

fn is_type(ty: &syn::Type, to_match: &str) -> bool {
    let syn::Type::Path(typepath) = ty else {
        return false
    };
    if typepath.qself.is_some() {
        return false
    }
    let Some(segment) = typepath.path.segments.last() else {
        return false;
    };

    segment.ident == to_match
}

#[proc_macro_derive(TryFromRpcValue, attributes(field_name))]
pub fn derive_from_rpcvalue(item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::DeriveInput);
    let struct_identifier = &input.ident;

    match &input.data {
        syn::Data::Struct(syn::DataStruct { fields, .. }) => {
            let mut struct_initializers = quote!{};
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
                        #identifier: get_key(#field_name).ok().and_then(|x| x.try_into().ok()),
                    });
                    rpcvalue_inserts.extend(quote!{
                        if let Some(val) = value.#identifier {
                            map.insert(#field_name.into(), val.into());
                        }
                    });
                } else {
                    struct_initializers.extend(quote!{
                        #identifier: get_key(#field_name).and_then(|x| x.try_into())?,
                    });
                    rpcvalue_inserts.extend(quote!{
                        map.insert(#field_name.into(), value.#identifier.into());
                    });
                }
            }
            quote!{
                impl TryFrom<shvproto::RpcValue> for #struct_identifier {
                    type Error = String;
                    fn try_from(value: shvproto::RpcValue) -> Result<Self, Self::Error> {
                        if let shvproto::Value::Map(value) = value.value() {
                            value.try_into()
                        } else {
                            Err("Value is not a map".into())
                        }
                    }
                }

                impl TryFrom<&shvproto::RpcValue> for #struct_identifier {
                    type Error = String;
                    fn try_from(value: &shvproto::RpcValue) -> Result<Self, Self::Error> {
                        if let shvproto::Value::Map(value) = value.value() {
                            value.as_ref().try_into()
                        } else {
                            Err("Value is not a map".into())
                        }
                    }
                }

                impl TryFrom<&Box<shvproto::Map>> for #struct_identifier {
                    type Error = String;
                    fn try_from(value: &Box<shvproto::Map>) -> Result<Self, Self::Error> {
                        value.as_ref().try_into()
                    }
                }

                impl TryFrom<&shvproto::Map> for #struct_identifier {
                    type Error = String;
                    fn try_from(value: &shvproto::Map) -> Result<Self, Self::Error> {
                        let get_key = |key_name| value.get(key_name).ok_or_else(|| "Missing ".to_string() + key_name + " key");
                        Ok(Self {
                            #struct_initializers
                        })
                    }
                }

                impl TryFrom<shvproto::Map> for #struct_identifier {
                    type Error = String;
                    fn try_from(value: shvproto::Map) -> Result<Self, Self::Error> {
                        Self::try_from(&value)
                    }
                }

                impl From<#struct_identifier> for shvproto::RpcValue {
                    fn from(value: #struct_identifier) -> Self {
                        let mut map = shvproto::rpcvalue::Map::new();
                        #rpcvalue_inserts
                        map.into()
                    }
                }
            }
        },
        syn::Data::Enum(syn::DataEnum { variants, .. }) => {
            let mut match_arms_de  = quote!{};
            let mut match_arms_ser = quote!{};
            let mut allowed_types = vec![];
            for variant in variants {
                let variant_ident = &variant.ident;
                let mut add_type_matcher = |should, dest_variant_type, block| {
                    if should {
                        allowed_types.push(quote!{stringify!(#dest_variant_type)});

                        match_arms_de.extend(quote!{
                            shvproto::Value::#dest_variant_type => Ok(<#struct_identifier>::#variant_ident #block),
                        });
                    }
                };
                if let syn::Fields::Unnamed(variant_types) = &variant.fields {
                    if variant_types.unnamed.len() != 1 {
                        panic!("Only single element variant tuples are supported for TryFromRpcValue");
                    }
                    match_arms_ser.extend(quote!{
                        #struct_identifier::#variant_ident(val) => RpcValue::from(val),
                    });

                    let source_variant_type = &variant_types.unnamed.first().expect("No tuple elements").ty;
                    let deref_code = quote!((*x));
                    let unbox_code = quote!((x.as_ref().clone()));

                    add_type_matcher(is_type(source_variant_type, "i64"), quote!{Int(x)}, deref_code.clone());
                    add_type_matcher(is_type(source_variant_type, "u64"), quote!{UInt(x)}, deref_code.clone());
                    add_type_matcher(is_type(source_variant_type, "f64"), quote!{Double(x)}, deref_code.clone());
                    add_type_matcher(is_type(source_variant_type, "bool"), quote!{Bool(x)}, deref_code.clone());
                    add_type_matcher(is_type(source_variant_type, "DateTime"), quote!{DateTime(x)}, deref_code.clone());
                    add_type_matcher(is_type(source_variant_type, "Decimal"), quote!{Decimal(x)}, deref_code.clone());
                    add_type_matcher(is_type(source_variant_type, "String"), quote!{String(x)}, unbox_code.clone());
                    add_type_matcher(is_type(source_variant_type, "Blob"), quote!{Blob(x)}, unbox_code.clone());
                    add_type_matcher(is_type(source_variant_type, "List"), quote!{List(x)}, unbox_code.clone());
                    add_type_matcher(is_type(source_variant_type, "Map"), quote!{Map(x)}, unbox_code.clone());
                    add_type_matcher(is_type(source_variant_type, "IMap"), quote!{IMap(x)}, unbox_code.clone());
                }
                if let syn::Fields::Unit = &variant.fields {
                    match_arms_ser.extend(quote!{
                        #struct_identifier::#variant_ident => RpcValue::null(),
                    });
                    add_type_matcher(true, quote! {Null}, quote!());
                }
            }

            quote!{
                impl TryFrom<shvproto::RpcValue> for #struct_identifier {
                    type Error = String;
                    fn try_from(value: shvproto::RpcValue) -> Result<Self, Self::Error> {
                        match value.value() {
                            #match_arms_de
                            _ => Err("Couldn't deserialize into '".to_owned() + stringify!(#struct_identifier) + "' enum, allowed types: " + [#(#allowed_types),*].join("|").as_ref() + ", got: " + value.type_name())
                        }
                    }
                }

                impl TryFrom<&shvproto::RpcValue> for #struct_identifier {
                    type Error = String;
                    fn try_from(value: &shvproto::RpcValue) -> Result<Self, Self::Error> {
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
        _ => panic!("This macro can only be used on a struct or a enum.")
    }.into()
}
