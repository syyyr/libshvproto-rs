#[cfg(test)]
mod test {
    use std::collections::BTreeMap;

    use libshvproto_macros::TryFromRpcValue;
    use shvproto::RpcValue;

    #[derive(Debug,PartialEq,TryFromRpcValue)]
    struct EmptyStruct {
    }

    impl From<EmptyStruct> for RpcValue {
        fn from(_value: EmptyStruct) -> Self {
            shvproto::make_map!().into()
        }
    }

    #[derive(Debug,PartialEq,TryFromRpcValue)]
    struct OneFieldStruct {
        x: i32
    }

    impl From<OneFieldStruct> for RpcValue {
        fn from(value: OneFieldStruct) -> Self {
            shvproto::make_map!("x" => value.x).into()
        }
    }

    #[derive(Debug,PartialEq,TryFromRpcValue)]
    struct TestStruct {
        int_field: i32,
        #[field_name = "my_custom_field_name"] int_field_with_custom_field_name: i32,
        string_field: String,
        map_field: shvproto::Map,
        empty_struct_field: EmptyStruct,
        one_field_struct: OneFieldStruct,
        vec_int_field: Vec<i32>,
        vec_empty_struct_field: Vec<EmptyStruct>,
        map_int_field: BTreeMap<String, i32>,
    }

    impl From<TestStruct> for RpcValue {
        fn from(value: TestStruct) -> Self {
            shvproto::make_map!(
                "intField" => value.int_field,
                "my_custom_field_name" => value.int_field_with_custom_field_name,
                "stringField" => value.string_field,
                "mapField" => value.map_field,
                "emptyStructField" => value.empty_struct_field,
                "oneFieldStruct" => value.one_field_struct,
                "vecIntField" => value.vec_int_field,
                "vecEmptyStructField" => value.vec_empty_struct_field,
                "mapIntField" => value.map_int_field
            ).into()
        }
    }

    #[test]
    fn derive_struct() {
        let x: RpcValue = shvproto::make_map!(
            "intField" => 123,
            "my_custom_field_name" => 1234,
            "stringField" => "some_string",
            "mapField" => shvproto::make_map!(
                "some_key" => 123
            ),
            "emptyStructField" => shvproto::make_map!(),
            "oneFieldStruct" => shvproto::make_map!("x" => 4565),
            "vecIntField" => vec![1_i32, 2_i32].into_iter().map(RpcValue::from).collect::<Vec<_>>(),
            "vecEmptyStructField" => vec![shvproto::make_map!(), shvproto::make_map!()].into_iter().map(RpcValue::from).collect::<Vec<_>>(),
            "mapIntField" => [("aaa".to_string(), 111)].into_iter().collect::<BTreeMap<_,_>>()
        ).into();

        let y: TestStruct = x.clone().try_into().expect("Failed to parse");

        assert_eq!(y, TestStruct {
            int_field: 123,
            string_field: "some_string".to_owned(),
            int_field_with_custom_field_name: 1234,
            map_field: shvproto::make_map!("some_key" => 123),
            one_field_struct: OneFieldStruct {x: 4565},
            empty_struct_field: EmptyStruct{},
            vec_int_field: vec![1_i32, 2_i32],
            vec_empty_struct_field: vec![EmptyStruct{}, EmptyStruct{}],
            map_int_field: [("aaa".to_string(), 111)].into_iter().collect::<BTreeMap<_,_>>(),
        });
        assert_eq!(x, y.into());
    }

    #[test]
    #[should_panic]
    fn missing_fields() {
        let _x: TestStruct = shvproto::make_map!(
            "my_custom_field_name" => 1234
        ).try_into().expect("Asd");
    }
}
