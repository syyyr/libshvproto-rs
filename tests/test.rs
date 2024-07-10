#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use shvproto::TryFromRpcValue;

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    pub struct EmptyStruct {
    }

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    pub struct OneFieldStruct {
        x: i32
    }

    #[derive(Debug,PartialEq,TryFromRpcValue)]
    pub struct TestStruct {
        int_field: i32,
        #[field_name = "myCustomFieldName"] int_field_with_custom_field_name: i32,
        string_field: String,
        map_field: shvproto::Map,
        empty_struct_field: EmptyStruct,
        one_field_struct: OneFieldStruct,
        vec_int_field: Vec<i32>,
        vec_empty_struct_field: Vec<EmptyStruct>,
        map_int_field: BTreeMap<String, i32>,
        imap_field: BTreeMap<i32, i32>,
    }

    #[derive(Debug,PartialEq,TryFromRpcValue)]
    struct OptionalFieldStruct {
        optional_int_field: Option<i32>
    }

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    pub enum AllVariants {
        Null,
        Int(i64),
        UInt(u64),
        Double(f64),
        Bool(bool),
        DateTime(shvproto::datetime::DateTime),
        Decimal(shvproto::decimal::Decimal),
        String(String),
        Blob(shvproto::Blob),
        List(shvproto::List),
        Map(shvproto::Map),
        IMap(shvproto::rpcvalue::IMap),
    }

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    pub enum EnumWithUserStruct {
        OneFieldStructVariant(OneFieldStruct),
        IntVariant(i64)
    }

    #[test]
    fn derive_struct() {
        let x: shvproto::RpcValue = shvproto::make_map!(
            "intField" => 123,
            "myCustomFieldName" => 1234,
            "stringField" => "some_string",
            "mapField" => shvproto::make_map!(
                "some_key" => 123
            ),
            "emptyStructField" => shvproto::make_map!(),
            "oneFieldStruct" => shvproto::make_map!("x" => 4565),
            "vecIntField" => vec![1_i32, 2_i32].into_iter().map(shvproto::RpcValue::from).collect::<Vec<_>>(),
            "vecEmptyStructField" => vec![shvproto::make_map!(), shvproto::make_map!()].into_iter().map(shvproto::RpcValue::from).collect::<Vec<_>>(),
            "mapIntField" => [("aaa".to_string(), 111)].into_iter().collect::<BTreeMap<_,_>>(),
            "imapField" => [(420, 111)].into_iter().collect::<BTreeMap<_,_>>(),
        ).into();

        let y: TestStruct = x.clone().try_into().expect("Failed to parse");
        assert_eq!(x, y.into());
    }

    #[test]
    #[should_panic]
    fn missing_fields() {
        let _x: TestStruct = shvproto::make_map!(
            "my_custom_field_name" => 1234
        ).try_into().expect("Expected parse failure");
    }

    #[test]
    fn optional_field() {
        let x: OptionalFieldStruct = shvproto::make_map!().try_into().expect("Failed to parse");
        assert_eq!(x, OptionalFieldStruct {
            optional_int_field: None
        });
        let y: OptionalFieldStruct = shvproto::make_map!(
            "optionalIntField" => 59
        ).try_into().expect("Failed to parse");
        assert_eq!(y, OptionalFieldStruct {
            optional_int_field: Some(59)
        });
    }

    #[test]
    fn enum_field() {
        let impl_test = |x: AllVariants| {
            let y: shvproto::RpcValue = x.clone().into();
            let z: AllVariants = y.try_into().expect("Failed to parse");
            assert_eq!(x, z);
        };

        impl_test(AllVariants::Null);
        impl_test(AllVariants::Int(123));
        impl_test(AllVariants::UInt(465));
        impl_test(AllVariants::Double(123.0));
        impl_test(AllVariants::Bool(true));
        impl_test(AllVariants::DateTime(shvproto::DateTime::now()));
        impl_test(AllVariants::Decimal(shvproto::Decimal::new(1234, 2)));
        impl_test(AllVariants::String("Some string".to_owned()));
        impl_test(AllVariants::Blob(vec![1, 2, 3]));
        impl_test(AllVariants::List(vec![shvproto::RpcValue::from("some_value")]));
        impl_test(AllVariants::Map(shvproto::make_map!("key" => 1234)));
        impl_test(AllVariants::IMap([(420, 111.into())].into_iter().collect::<BTreeMap<_,_>>()));

        let impl_test = |x: EnumWithUserStruct| {
            let y: shvproto::RpcValue = x.clone().into();
            let z: EnumWithUserStruct = y.try_into().expect("Failed to parse");
            assert_eq!(x, z);
        };

        impl_test(EnumWithUserStruct::OneFieldStructVariant(OneFieldStruct{x: 123}));
        impl_test(EnumWithUserStruct::IntVariant(123));
    }

    #[should_panic]
    #[test]
    fn cant_deser_enum() {
        let input: shvproto::RpcValue = String::new().into();
        let _output: EnumWithUserStruct = input.try_into().expect("Expected failure");
    }

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    pub struct GenericStruct<T> {
        x: T,
        y: Vec<T>
    }

    #[test]
    fn generic_struct() {
        let int_struct_in: GenericStruct::<i64> = shvproto::make_map!("x" => 123, "y" => vec![123, 456]).try_into().expect("Failed to parse");
        let int_struct_rpcvalue: shvproto::RpcValue = int_struct_in.clone().into();
        let int_struct_out: GenericStruct::<i64> = int_struct_rpcvalue.clone().try_into().expect("Failed to parse");
        assert_eq!(int_struct_in, int_struct_out);

    }
}
