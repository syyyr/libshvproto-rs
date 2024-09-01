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

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    pub struct TwoFieldsStruct {
        x: i32,
        y: f64,
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
        Null(()),
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

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    pub enum EnumWithMoreUserStructs {
        Null(()),
        EmptyStructVariant(EmptyStruct),
        OneFieldStructVariant(OneFieldStruct),
        TwoFieldsStructVariant(TwoFieldsStruct),
        IntVariant(i64),
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
    #[should_panic]
    fn unexpected_field() {
        let _: OneFieldStruct = shvproto::make_map!(
            "x" => 1234,
            "extra" => 33,
        ).try_into().unwrap();
    }

    #[test]
    #[should_panic]
    fn unexpected_field_empty_struct() {
        let _: EmptyStruct = shvproto::make_map!(
            "extra" => 33,
        ).try_into().unwrap();
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
    #[should_panic]
    fn optional_field_failing() {
        let _x: OptionalFieldStruct = shvproto::make_map!(
            "optionalIntField" => "bar"
        ).try_into().expect("Failed to parse");
    }

    fn test_case<T>(v: T)
    where
        T: TryFrom<shvproto::RpcValue> + Into<shvproto::RpcValue> + std::fmt::Debug + Clone + PartialEq,
        <T as TryFrom<shvproto::RpcValue>>::Error: std::fmt::Debug + PartialEq,
    {
        let rv: shvproto::RpcValue = v.clone().into();
        assert_eq!(Ok(v), rv.try_into());
    }

    #[test]
    fn enum_more_user_structs() {
        test_case(EnumWithMoreUserStructs::Null(()));
        test_case(EnumWithMoreUserStructs::OneFieldStructVariant(OneFieldStruct { x: 42 }));
        test_case(EnumWithMoreUserStructs::TwoFieldsStructVariant(TwoFieldsStruct { x: 1, y: 1.23 }));
        test_case(EnumWithMoreUserStructs::EmptyStructVariant(EmptyStruct { }));
        test_case(EnumWithMoreUserStructs::IntVariant(-1));
    }

    #[test]
    #[should_panic]
    fn enum_more_user_structs_failing() {
        let _: EnumWithMoreUserStructs = shvproto::RpcValue::from(shvproto::make_map!("y" => 1.23_f64)).try_into().unwrap();
    }

    #[test]
    fn enum_field() {
        test_case(AllVariants::Null(()));
        test_case(AllVariants::Int(123));
        test_case(AllVariants::UInt(465));
        test_case(AllVariants::Double(123.0));
        test_case(AllVariants::Bool(true));
        test_case(AllVariants::DateTime(shvproto::DateTime::now()));
        test_case(AllVariants::Decimal(shvproto::Decimal::new(1234, 2)));
        test_case(AllVariants::String("Some string".to_owned()));
        test_case(AllVariants::Blob(vec![1, 2, 3]));
        test_case(AllVariants::List(vec![shvproto::RpcValue::from("some_value")]));
        test_case(AllVariants::Map(shvproto::make_map!("key" => 1234)));
        test_case(AllVariants::IMap([(420, 111.into())].into_iter().collect::<BTreeMap<_,_>>()));

        test_case(EnumWithUserStruct::OneFieldStructVariant(OneFieldStruct{x: 123}));
        test_case(EnumWithUserStruct::IntVariant(123));
    }

    #[should_panic]
    #[test]
    fn cant_deser_enum() {
        let input: shvproto::RpcValue = String::new().into();
        let _output: EnumWithUserStruct = input.try_into().expect("Expected failure");
    }

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    pub struct GenericStruct<Type1, Type2> {
        x: Type1,
        y: Vec<Type1>,
        z: Type2
    }

    #[test]
    fn generic_struct() {
        let int_struct_in: GenericStruct::<i64, String> = shvproto::make_map!("x" => 123, "y" => vec![123, 456], "z" => "some_string").try_into().expect("Failed to parse");
        let int_struct_rpcvalue: shvproto::RpcValue = int_struct_in.clone().into();
        let int_struct_out: GenericStruct::<i64, String> = int_struct_rpcvalue.clone().try_into().expect("Failed to parse");
        assert_eq!(int_struct_in, int_struct_out);

    }

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    enum UnitVariantsEnum {
        Variant1,
        Variant2,
        Error,
        NoValue(()),
        Str(String),
        Int(i64),
    }

    #[test]
    fn unit_variants_enum() {
        test_case(UnitVariantsEnum::Variant1);
        test_case(UnitVariantsEnum::Variant2);
        test_case(UnitVariantsEnum::Error);
        test_case(UnitVariantsEnum::NoValue(()));
        test_case(UnitVariantsEnum::Str("foo".into()));
        test_case(UnitVariantsEnum::Int(32));

    }

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    enum UnitVariantsOnlyEnum {
        Variant1,
        Variant2,
    }

    #[test]
    #[should_panic]
    fn unit_variants_enum_failing() {
        let rv = shvproto::RpcValue::from("foo");
        let _v: UnitVariantsOnlyEnum = rv.try_into().unwrap();
    }

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    pub enum EnumWithNamedFields {
        Linux { shell: String, user: String, uptime_days: i32, },
        MacOsX { shell: String, user: String, uptime_days: i32, },
        Windows { user: Option<String>, number_of_failures: i64 },
    }

    #[test]
    fn enum_with_named_fields() {
        test_case(EnumWithNamedFields::Linux { user: "alice".to_string(), shell: "bash".to_string(), uptime_days: 888 });
        test_case(EnumWithNamedFields::MacOsX { shell: "zsh".to_string(), user: "bob".to_string(), uptime_days: 666, });
        test_case(EnumWithNamedFields::Windows { user: Some("boomer".to_string()), number_of_failures: 12 << 33 });
    }

    #[test]
    #[should_panic]
    fn enum_with_named_fields_invalid_type_failing() {
        let rv = shvproto::RpcValue::from("foo");
        let _v: EnumWithNamedFields = rv.try_into().unwrap();
    }

    #[test]
    #[should_panic]
    fn enum_with_named_fields_missing_tag_failing() {
        let rv: shvproto::RpcValue = shvproto::make_map!{
            "user" => "alice",
            "shell" => "csh",
            "uptimeDays" => 1,
        }.into();
        let _v: EnumWithNamedFields = rv.try_into().unwrap();
    }

    #[test]
    #[should_panic]
    fn enum_with_named_fields_missing_field_failing() {
        let rv: shvproto::RpcValue = shvproto::make_map!{
            "type" => "macOsX",
            "user" => "alice",
            "uptimeDays" => 1,
        }.into();
        let _v: EnumWithNamedFields = rv.try_into().unwrap();
    }

    #[test]
    #[should_panic]
    fn enum_with_named_fields_field_type_mismatch_failing() {
        let rv: shvproto::RpcValue = shvproto::make_map!{
            "type" => "macOsX",
            "user" => "alice",
            "shell" => vec!["bash", "sh"],
            "uptimeDays" => 1,
        }.into();
        let _v: EnumWithNamedFields = rv.try_into().unwrap();
    }

    #[test]
    fn enum_with_named_fields_tryinto() {
        let rv: shvproto::RpcValue = shvproto::make_map!{
            "linux" => shvproto::make_map!{
                "user" => "alice",
                "shell" => "bash",
                "uptimeDays" => 10,
            },
        }.into();
        let _v: EnumWithNamedFields = rv.try_into().unwrap();
    }

    #[derive(Clone,Debug,PartialEq,TryFromRpcValue)]
    #[rpcvalue(tag = "os")]
    pub enum EnumWithNamedFieldsCustomTag {
        Linux { shell: String, user: String, uptime_days: i32, },
        MacOsX { shell: String, user: String, uptime_days: i32, },
        Windows { user: Option<String>, number_of_failures: i64 },
    }

    #[test]
    fn enum_with_named_fields_custom_tag() {
        test_case(EnumWithNamedFieldsCustomTag::Linux { user: "alice".to_string(), shell: "bash".to_string(), uptime_days: 888 });
        test_case(EnumWithNamedFieldsCustomTag::MacOsX { shell: "zsh".to_string(), user: "bob".to_string(), uptime_days: 666, });
        test_case(EnumWithNamedFieldsCustomTag::Windows { user: Some("boomer".to_string()), number_of_failures: 12 << 33 });
    }

    #[test]
    fn enum_with_named_fields_custom_tag_tryinto() {
        let rv: shvproto::RpcValue = shvproto::make_map!{
            "os" => "linux",
            "user" => "alice",
            "shell" => "bash",
            "uptimeDays" => 10,
        }.into();
        let _v: EnumWithNamedFieldsCustomTag = rv.try_into().unwrap();
    }

    #[test]
    #[should_panic]
    fn enum_with_named_fields_custom_tag_type_mismatch() {
        let rv: shvproto::RpcValue = shvproto::make_map!{
            "os" => 0,
            "user" => "alice",
            "shell" => "bash",
            "uptimeDays" => 10,
        }.into();
        let _v: EnumWithNamedFieldsCustomTag = rv.try_into().unwrap();
    }

    #[test]
    #[should_panic]
    fn enum_with_named_fields_custom_tag_missing() {
        let rv: shvproto::RpcValue = shvproto::make_map!{
            "type" => "linux",
            "user" => "alice",
            "shell" => "bash",
            "uptimeDays" => 10,
        }.into();
        let _v: EnumWithNamedFieldsCustomTag = rv.try_into().unwrap();
    }
}
