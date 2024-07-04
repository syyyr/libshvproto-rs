use convert_case::{Case, Casing};

use core::panic;

use proc_macro::TokenStream;
use quote::quote;

#[proc_macro_derive(TryFromRpcValue, attributes(field_name))]
pub fn derive_from_rpcvalue(item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::DeriveInput);
    let struct_identifier = &input.ident;

    match &input.data {
        syn::Data::Struct(syn::DataStruct { fields, .. }) => {
            let mut struct_initializers = quote!{};
            for field in fields {
                let identifier = field.ident.as_ref().unwrap();
                let field_name = field
                    .attrs.first()
                    .and_then(|x| x.meta.require_name_value().ok())
                    .filter(|x| x.path.is_ident("field_name"))
                    .map(|x| if let syn::Expr::Lit(expr) = &x.value { expr } else { panic!("Expected a string literal for 'field_name'") })
                    .map(|x| if let syn::Lit::Str(expr) = &x.lit { expr.value() } else { panic!("Expected a string literal for 'field_name'") })
                    .unwrap_or_else(|| identifier.to_string().to_case(Case::Camel));

                struct_initializers.extend(quote!{
                    #identifier: get_key(#field_name).and_then(|x| x.try_into())?,
                });

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
                        let get_key = |key_name| value.get(key_name).ok_or_else(|| "Missing ".to_string() + key_name + " key");
                        Ok(Self {
                            #struct_initializers
                        })
                    }
                }

            }
        }
        _ => panic!("This macro can only be used on a struct.")
    }.into()
}
