#[cfg(test)]
mod test {
    use libshvproto_macros::TryFromRpcValue;
    use shvproto::RpcValue;
    use shvproto::rpcvalue;

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    struct EmptyStruct {
    }

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    struct OneFieldStruct {
        x: i32
    }

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    struct TestStruct {
        int_field: i32,
        #[field_name = "my_custom_field_name"] int_field_with_custom_field_name: i32,
        string_field: String,
        map_field: shvproto::Map,
        empty_struct_field: EmptyStruct,
        one_field_struct: OneFieldStruct
    }

    #[test]
    fn derive_struct() {
        let input_map = shvproto::make_map!(
            "intField" => 123,
            "my_custom_field_name" => 1234,
            "stringField" => "some_string",
            "mapField" => shvproto::make_map!(
                "some_key" => 123
            ),
            "emptyStructField" => shvproto::make_map!(),
            "oneFieldStruct" => shvproto::make_map!("x" => 4565)
        );
        let parsed_struct: TestStruct = (&input_map).try_into().expect("Failed to parse");
        let output_map: RpcValue = parsed_struct.into();
        assert_eq!(output_map.as_map(), &input_map);
    }

    #[test]
    #[should_panic]
    fn missing_fields() {
        let _x: TestStruct = shvproto::make_map!(
            "my_custom_field_name" => 1234
        ).try_into().expect("Asd");
    }
}
