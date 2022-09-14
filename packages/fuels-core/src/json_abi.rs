use crate::code_gen::abigen::Abigen;
use crate::code_gen::function_selector::resolve_fn_selector;
use crate::tokenizer::Tokenizer;
use crate::utils::first_four_bytes_of_sha256_hash;
use crate::Token;
use crate::{abi_decoder::ABIDecoder, abi_encoder::ABIEncoder};
use fuels_types::ProgramABI;
use fuels_types::{errors::Error, param_types::ParamType};
use serde_json;
use std::str;

pub struct ABIParser {
    fn_selector: Option<Vec<u8>>,
}

impl Default for ABIParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ABIParser {
    pub fn new() -> Self {
        ABIParser { fn_selector: None }
    }

    /// Higher-level layer of the ABI encoding module.
    /// Encode is essentially a wrapper of [`crate::abi_encoder`],
    /// but it is responsible for parsing strings into proper [`Token`]
    /// that can be encoded by the [`crate::abi_encoder`].
    /// Note that `encode` only encodes the parameters for an ABI call,
    /// It won't include the function selector in it. To get the function
    /// selector, use `encode_with_function_selector`.
    ///
    /// # Examples (@todo update doctest)
    /// ```no_run
    /// use fuels_core::json_abi::ABIParser;
    /// let json_abi = r#"
    ///     [
    ///         {
    ///             "type":"contract",
    ///             "inputs":[
    ///                 {
    ///                     "name":"arg",
    ///                     "type":"u32"
    ///                 }
    ///             ],
    ///             "name":"takes_u32_returns_bool",
    ///             "outputs":[
    ///                 {
    ///                     "name":"",
    ///                     "type":"bool"
    ///                 }
    ///             ]
    ///         }
    ///     ]
    ///     "#;
    ///
    ///     let values: Vec<String> = vec!["10".to_string()];
    ///
    ///     let mut abi = ABIParser::new();
    ///
    ///     let function_name = "takes_u32_returns_bool";
    ///     let encoded = abi.encode(json_abi, function_name, &values).unwrap();
    ///     let expected_encode = "000000000000000a";
    ///     assert_eq!(encoded, expected_encode);
    /// ```
    pub fn encode(&mut self, abi: &str, fn_name: &str, values: &[String]) -> Result<String, Error> {
        let parsed_abi: ProgramABI = serde_json::from_str(abi)?;

        let entry = parsed_abi.functions.iter().find(|e| e.name == fn_name);

        let entry = entry.expect("No functions found");

        let types = Abigen::get_types(&parsed_abi);

        let fn_selector = resolve_fn_selector(entry, &types);

        // Update the fn_selector field with the hash of the previously encoded function selector
        self.fn_selector = Some(first_four_bytes_of_sha256_hash(&fn_selector).to_vec());

        let params_and_values = entry
            .inputs
            .iter()
            .zip(values)
            .map(|(prop, val)| {
                let t = types.get(&prop.type_id).unwrap();
                Ok((ParamType::from_type_declaration(t, &types)?, val.as_str()))
            })
            .collect::<Result<Vec<_>, Error>>()?;

        let tokens = self.parse_tokens(&params_and_values)?;

        Ok(hex::encode(ABIEncoder::encode(&tokens)?))
    }

    /// Similar to `encode`, but includes the function selector in the
    /// final encoded string.
    ///
    /// # Examples (@todo update doctest)
    /// ```no_run
    /// use fuels_core::json_abi::ABIParser;
    /// let json_abi = r#"
    ///     [
    ///         {
    ///             "type":"contract",
    ///             "inputs":[
    ///                 {
    ///                     "name":"arg",
    ///                     "type":"u32"
    ///                 }
    ///             ],
    ///             "name":"takes_u32_returns_bool",
    ///             "outputs":[
    ///                 {
    ///                     "name":"",
    ///                     "type":"bool"
    ///                 }
    ///             ]
    ///         }
    ///     ]
    ///     "#;
    ///
    ///     let values: Vec<String> = vec!["10".to_string()];
    ///
    ///     let mut abi = ABIParser::new();
    ///     let function_name = "takes_u32_returns_bool";
    ///
    ///     let encoded = abi
    ///         .encode_with_function_selector(json_abi, function_name, &values)
    ///         .unwrap();
    ///
    ///     let expected_encode = "000000006355e6ee000000000000000a";
    ///     assert_eq!(encoded, expected_encode);
    /// ```
    pub fn encode_with_function_selector(
        &mut self,
        abi: &str,
        fn_name: &str,
        values: &[String],
    ) -> Result<String, Error> {
        let encoded_params = self.encode(abi, fn_name, values)?;
        let fn_selector = self
            .fn_selector
            .to_owned()
            .expect("Function selector not encoded");

        let encoded_function_selector = hex::encode(fn_selector);

        Ok(format!("{}{}", encoded_function_selector, encoded_params))
    }

    /// Similar to `encode`, but it encodes only an array of strings containing
    /// [<type_1>, <param_1>, <type_2>, <param_2>, <type_n>, <param_n>]
    /// Without having to reference to a JSON specification of the ABI.
    /// TODO: This is currently disabled because it needs to be updated to the
    /// new ABI spec.
    // pub fn encode_params(&self, params: &[String]) -> Result<String, Error> {
    //     let pairs: Vec<_> = params.chunks(2).collect_vec();

    //     let mut param_type_pairs: Vec<(ParamType, &str)> = vec![];

    //     for pair in pairs {
    //         let prop = Property {
    //             name: "".to_string(),
    //             type_field: pair[0].clone(),
    //             components: None,
    //         };
    //         let p = ParamType::try_from(&prop)?;

    //         let t: (ParamType, &str) = (p, &pair[1]);
    //         param_type_pairs.push(t);
    //     }

    //     let tokens = self.parse_tokens(&param_type_pairs)?;

    //     let encoded = ABIEncoder::encode(&tokens)?;

    //     Ok(hex::encode(encoded))
    // }

    /// Helper function to turn a list of tuples(ParamType, &str) into
    /// a vector of Tokens ready to be encoded.
    /// Essentially a wrapper on `tokenize`.
    pub fn parse_tokens<'a>(&self, params: &'a [(ParamType, &str)]) -> Result<Vec<Token>, Error> {
        params
            .iter()
            .map(|&(ref param, value)| Tokenizer::tokenize(param, value.to_string()))
            .collect::<Result<_, _>>()
            .map_err(From::from)
    }

    /// Higher-level layer of the ABI decoding module.
    /// Decodes a value of a given ABI and a target function's output.
    /// Note that the `value` has to be a byte array, meaning that
    /// the caller must properly cast the "upper" type into a `&[u8]`,
    pub fn decode<'a>(
        &self,
        abi: &str,
        fn_name: &str,
        value: &'a [u8],
    ) -> Result<Vec<Token>, Error> {
        let parsed_abi: ProgramABI = serde_json::from_str(abi)?;

        let entry = parsed_abi.functions.iter().find(|e| e.name == fn_name);

        if entry.is_none() {
            return Err(Error::InvalidData(format!(
                "couldn't find function name: {}",
                fn_name
            )));
        }

        let types = Abigen::get_types(&parsed_abi);

        let param_result = types
            .get(&entry.unwrap().output.type_id)
            .expect("No output type");

        let param_result = ParamType::from_type_declaration(param_result, &types);

        match param_result {
            Ok(params) => Ok(ABIDecoder::decode(&[params], value)?),
            Err(e) => Err(e),
        }
    }

    /// Similar to decode, but it decodes only an array types and the encoded data
    /// without having to reference to a JSON specification of the ABI.
    pub fn decode_params(&self, params: &[ParamType], data: &[u8]) -> Result<Vec<Token>, Error> {
        Ok(ABIDecoder::decode(params, data)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::StringToken;
    use fuels_types::errors::Error;

    #[test]
    fn simple_encode_and_decode_no_selector() -> Result<(), Error> {
        let json_abi = r#"
        {
            "types": [
                {
                    "typeId": 0,
                    "type": "bool",
                    "components": null,
                    "typeParameters": null
                },
                {
                    "typeId": 1,
                    "type": "u32",
                    "components": null,
                    "typeParameters": null
                }
            ],
            "functions": [
                {
                    "inputs": [
                        {
                            "name": "only_argument",
                            "type": 1,
                            "typeArguments": null
                        }
                    ],
                    "name": "takes_u32_returns_bool",
                    "output": {
                        "name": "",
                        "type": 0,
                        "typeArguments": null
                    }
                }
            ]
        }
        "#;

        let values: Vec<String> = vec!["10".to_string()];

        let mut abi = ABIParser::new();

        let function_name = "takes_u32_returns_bool";

        let encoded = abi.encode(json_abi, function_name, &values)?;

        let expected_encode = "000000000000000a";
        assert_eq!(encoded, expected_encode);

        let return_value = [
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, // false
        ];

        let decoded_return = abi.decode(json_abi, function_name, &return_value)?;

        let expected_return = vec![Token::Bool(false)];

        assert_eq!(decoded_return, expected_return);
        Ok(())
    }

    #[test]
    fn simple_encode_and_decode() -> Result<(), Error> {
        let json_abi = r#"
        {
            "types": [
                {
                    "typeId": 0,
                    "type": "bool",
                    "components": null,
                    "typeParameters": null
                },
                {
                    "typeId": 1,
                    "type": "u32",
                    "components": null,
                    "typeParameters": null
                }
            ],
            "functions": [
                {
                    "inputs": [
                        {
                            "name": "only_argument",
                            "type": 1,
                            "typeArguments": null
                        }
                    ],
                    "name": "takes_u32_returns_bool",
                    "output": {
                        "name": "",
                        "type": 0,
                        "typeArguments": null
                    }
                }
            ]
        }
        "#;

        let values: Vec<String> = vec!["10".to_string()];

        let mut abi = ABIParser::new();

        let function_name = "takes_u32_returns_bool";

        let encoded = abi.encode_with_function_selector(json_abi, function_name, &values)?;

        let expected_encode = "000000006355e6ee000000000000000a";
        assert_eq!(encoded, expected_encode);

        let return_value = [
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, // false
        ];

        let decoded_return = abi.decode(json_abi, function_name, &return_value)?;

        let expected_return = vec![Token::Bool(false)];

        assert_eq!(decoded_return, expected_return);
        Ok(())
    }

    #[test]
    fn b256_and_single_byte_encode_and_decode() -> Result<(), Box<dyn std::error::Error>> {
        let json_abi = r#"
        {
            "types": [
              {
                "typeId": 0,
                "type": "b256",
                "components": null,
                "typeParameters": null
              },
              {
                "typeId": 1,
                "type": "byte",
                "components": null,
                "typeParameters": null
              }
            ],
            "functions": [
              {
                "inputs": [
                  {
                    "name": "foo",
                    "type": 0,
                    "typeArguments": null
                  },
                  {
                    "name": "bar",
                    "type": 1,
                    "typeArguments": null
                  }
                ],
                "name": "my_func",
                "output": {
                  "name": "",
                  "type": 0,
                  "typeArguments": null
                }
              }
            ]
          }
        "#;

        let values: Vec<String> = vec![
            "d5579c46dfcc7f18207013e65b44e4cb4e2c2298f4ac457ba8f82743f31e930b".to_string(),
            "1".to_string(),
        ];

        let mut abi = ABIParser::new();

        let function_name = "my_func";

        let encoded = abi.encode_with_function_selector(json_abi, function_name, &values)?;

        let expected_encode = "00000000e64019abd5579c46dfcc7f18207013e65b44e4cb4e2c2298f4ac457ba8f82743f31e930b0000000000000001";
        assert_eq!(encoded, expected_encode);

        let return_value =
            hex::decode("a441b15fe9a3cf56661190a0b93b9dec7d04127288cc87250967cf3b52894d11")?;

        let decoded_return = abi.decode(json_abi, function_name, &return_value)?;

        let s: [u8; 32] = return_value.as_slice().try_into()?;

        let expected_return = vec![Token::B256(s)];

        assert_eq!(decoded_return, expected_return);
        Ok(())
    }

    #[test]
    fn array_encode_and_decode() -> Result<(), Error> {
        let json_abi = r#"
        {
            "types": [
              {
                "typeId": 0,
                "type": "[_; 2]",
                "components": [
                  {
                    "name": "__array_element",
                    "type": 2,
                    "typeArguments": null
                  }
                ],
                "typeParameters": null
              },
              {
                "typeId": 1,
                "type": "[_; 3]",
                "components": [
                  {
                    "name": "__array_element",
                    "type": 2,
                    "typeArguments": null
                  }
                ],
                "typeParameters": null
              },
              {
                "typeId": 2,
                "type": "u16",
                "components": null,
                "typeParameters": null
              }
            ],
            "functions": [
              {
                "inputs": [
                  {
                    "name": "arg",
                    "type": 1,
                    "typeArguments": null
                  }
                ],
                "name": "takes_array",
                "output": {
                  "name": "",
                  "type": 0,
                  "typeArguments": null
                }
              }
            ]
          }
        "#;

        let values: Vec<String> = vec!["[1,2,3]".to_string()];

        let mut abi = ABIParser::new();

        let function_name = "takes_array";

        let encoded = abi.encode_with_function_selector(json_abi, function_name, &values)?;

        let expected_encode = "00000000101cbeb5000000000000000100000000000000020000000000000003";
        assert_eq!(encoded, expected_encode);

        let return_value = [
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, // 0
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1, // 1
        ];

        let decoded_return = abi.decode(json_abi, function_name, &return_value)?;

        let expected_return = vec![Token::Array(vec![Token::U16(0), Token::U16(1)])];

        assert_eq!(decoded_return, expected_return);
        Ok(())
    }

    #[test]
    fn nested_array_encode_and_decode() -> Result<(), Error> {
        let json_abi = r#"
        {
            "types": [
              {
                "typeId": 0,
                "type": "[_; 2]",
                "components": [
                  {
                    "name": "__array_element",
                    "type": 2,
                    "typeArguments": null
                  }
                ],
                "typeParameters": null
              },
              {
                "typeId": 1,
                "type": "[_; 3]",
                "components": [
                  {
                    "name": "__array_element",
                    "type": 2,
                    "typeArguments": null
                  }
                ],
                "typeParameters": null
              },
              {
                "typeId": 2,
                "type": "u16",
                "components": null,
                "typeParameters": null
              }
            ],
            "functions": [
              {
                "inputs": [
                  {
                    "name": "arg",
                    "type": 1,
                    "typeArguments": null
                  }
                ],
                "name": "takes_nested_array",
                "output": {
                  "name": "",
                  "type": 0,
                  "typeArguments": null
                }
              }
            ]
          }
        "#;

        let values: Vec<String> = vec!["[[1,2],[3],[4]]".to_string()];

        let mut abi = ABIParser::new();

        let function_name = "takes_nested_array";

        let encoded = abi.encode_with_function_selector(json_abi, function_name, &values)?;

        let expected_encode =
            "00000000e6a030f00000000000000001000000000000000200000000000000030000000000000004";
        assert_eq!(encoded, expected_encode);

        let return_value = [
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, // 0
            0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, 0x1, // 1
        ];

        let decoded_return = abi.decode(json_abi, function_name, &return_value)?;

        let expected_return = vec![Token::Array(vec![Token::U16(0), Token::U16(1)])];

        assert_eq!(decoded_return, expected_return);
        Ok(())
    }

    #[test]
    fn string_encode_and_decode() -> Result<(), Error> {
        let json_abi = r#"
        {
            "types": [
              {
                "typeId": 0,
                "type": "str[2]",
                "components": [],
                "typeParameters": null
              },
              {
                "typeId": 1,
                "type": "str[23]",
                "components": null,
                "typeParameters": null
              }
            ],
            "functions": [
              {
                "inputs": [
                  {
                    "name": "arg",
                    "type": 1,
                    "typeArguments": null
                  }
                ],
                "name": "takes_string",
                "output": {
                  "name": "",
                  "type": 0,
                  "typeArguments": null
                }
              }
            ]
          }
        "#;

        let values: Vec<String> = vec!["This is a full sentence".to_string()];

        let mut abi = ABIParser::new();

        let function_name = "takes_string";

        let encoded = abi.encode_with_function_selector(json_abi, function_name, &values)?;

        let expected_encode = "00000000d56e76515468697320697320612066756c6c2073656e74656e636500";
        assert_eq!(encoded, expected_encode);

        let return_value = [
            0x4f, 0x4b, 0x0, 0x0, 0x0, 0x0, 0x0, 0x0, // "OK" encoded in utf8
        ];

        let decoded_return = abi.decode(json_abi, function_name, &return_value)?;

        let expected_return = vec![Token::String(StringToken::new("OK".into(), 2))];

        assert_eq!(decoded_return, expected_return);
        Ok(())
    }

    #[test]
    fn struct_encode_and_decode() -> Result<(), Error> {
        let json_abi = r#"
        {
            "types": [
              {
                "typeId": 0,
                "type": "()",
                "components": [],
                "typeParameters": null
              },
              {
                "typeId": 1,
                "type": "bool",
                "components": null,
                "typeParameters": null
              },
              {
                "typeId": 2,
                "type": "struct MyStruct",
                "components": [
                  {
                    "name": "foo",
                    "type": 3,
                    "typeArguments": null
                  },
                  {
                    "name": "bar",
                    "type": 1,
                    "typeArguments": null
                  }
                ],
                "typeParameters": null
              },
              {
                "typeId": 3,
                "type": "u8",
                "components": null,
                "typeParameters": null
              }
            ],
            "functions": [
              {
                "inputs": [
                  {
                    "name": "my_val",
                    "type": 2,
                    "typeArguments": null
                  }
                ],
                "name": "takes_struct",
                "output": {
                  "name": "",
                  "type": 0,
                  "typeArguments": null
                }
              }
            ]
          }
        "#;

        let values: Vec<String> = vec!["(42, true)".to_string()];

        let mut abi = ABIParser::new();

        let function_name = "takes_struct";

        let encoded = abi.encode_with_function_selector(json_abi, function_name, &values)?;

        let expected_encode = "00000000cb0b2f05000000000000002a0000000000000001";
        assert_eq!(encoded, expected_encode);
        Ok(())
    }

    #[test]
    fn struct_and_primitive_encode_and_decode() -> Result<(), Error> {
        let json_abi = r#"
        {
            "types": [
              {
                "typeId": 0,
                "type": "()",
                "components": [],
                "typeParameters": null
              },
              {
                "typeId": 1,
                "type": "bool",
                "components": null,
                "typeParameters": null
              },
              {
                "typeId": 2,
                "type": "struct MyStruct",
                "components": [
                  {
                    "name": "foo",
                    "type": 4,
                    "typeArguments": null
                  },
                  {
                    "name": "bar",
                    "type": 1,
                    "typeArguments": null
                  }
                ],
                "typeParameters": null
              },
              {
                "typeId": 3,
                "type": "u32",
                "components": null,
                "typeParameters": null
              },
              {
                "typeId": 4,
                "type": "u8",
                "components": null,
                "typeParameters": null
              }
            ],
            "functions": [
              {
                "inputs": [
                  {
                    "name": "my_struct",
                    "type": 2,
                    "typeArguments": null
                  },
                  {
                    "name": "foo",
                    "type": 3,
                    "typeArguments": null
                  }
                ],
                "name": "takes_struct_and_primitive",
                "output": {
                  "name": "",
                  "type": 0,
                  "typeArguments": null
                }
              }
            ]
          }
        "#;

        let values: Vec<String> = vec!["(42, true)".to_string(), "10".to_string()];

        let mut abi = ABIParser::new();

        let function_name = "takes_struct_and_primitive";

        let encoded = abi.encode_with_function_selector(json_abi, function_name, &values)?;

        let expected_encode = "000000005c445838000000000000002a0000000000000001000000000000000a";
        assert_eq!(encoded, expected_encode);
        Ok(())
    }

    #[test]
    fn nested_struct_encode_and_decode() -> Result<(), Error> {
        let json_abi = r#"
        {
            "types": [
              {
                "typeId": 0,
                "type": "()",
                "components": [],
                "typeParameters": null
              },
              {
                "typeId": 1,
                "type": "[_; 2]",
                "components": [
                  {
                    "name": "__array_element",
                    "type": 6,
                    "typeArguments": null
                  }
                ],
                "typeParameters": null
              },
              {
                "typeId": 2,
                "type": "bool",
                "components": null,
                "typeParameters": null
              },
              {
                "typeId": 3,
                "type": "struct MyNestedStruct",
                "components": [
                  {
                    "name": "x",
                    "type": 5,
                    "typeArguments": null
                  },
                  {
                    "name": "inner",
                    "type": 4,
                    "typeArguments": null
                  }
                ],
                "typeParameters": null
              },
              {
                "typeId": 4,
                "type": "struct Y",
                "components": [
                  {
                    "name": "a",
                    "type": 2,
                    "typeArguments": null
                  },
                  {
                    "name": "b",
                    "type": 1,
                    "typeArguments": null
                  }
                ],
                "typeParameters": null
              },
              {
                "typeId": 5,
                "type": "u16",
                "components": null,
                "typeParameters": null
              },
              {
                "typeId": 6,
                "type": "u8",
                "components": null,
                "typeParameters": null
              }
            ],
            "functions": [
              {
                "inputs": [
                  {
                    "name": "top_value",
                    "type": 3,
                    "typeArguments": null
                  }
                ],
                "name": "takes_nested_struct",
                "output": {
                  "name": "",
                  "type": 0,
                  "typeArguments": null
                }
              }
            ]
          }
        "#;

        let values: Vec<String> = vec!["(10, (true, [1,2]))".to_string()];

        let mut abi = ABIParser::new();

        let function_name = "takes_nested_struct";

        let encoded = abi.encode_with_function_selector(json_abi, function_name, &values)?;

        let expected_encode =
            "00000000b1fbe7e3000000000000000a000000000000000100000000000000010000000000000002";
        assert_eq!(encoded, expected_encode);

        Ok(())
    }

    #[test]
    fn tuple_encode_and_decode() -> Result<(), Error> {
        let json_abi = r#"
        {
            "types": [
              {
                "typeId": 0,
                "type": "()",
                "components": [],
                "typeParameters": null
              },
              {
                "typeId": 1,
                "type": "(_, _)",
                "components": [
                  {
                    "name": "__tuple_element",
                    "type": 3,
                    "typeArguments": null
                  },
                  {
                    "name": "__tuple_element",
                    "type": 2,
                    "typeArguments": null
                  }
                ],
                "typeParameters": null
              },
              {
                "typeId": 2,
                "type": "bool",
                "components": null,
                "typeParameters": null
              },
              {
                "typeId": 3,
                "type": "u64",
                "components": null,
                "typeParameters": null
              }
            ],
            "functions": [
              {
                "inputs": [
                  {
                    "name": "input",
                    "type": 1,
                    "typeArguments": null
                  }
                ],
                "name": "takes_tuple",
                "output": {
                  "name": "",
                  "type": 0,
                  "typeArguments": null
                }
              }
            ]
          }
        "#;

        let values: Vec<String> = vec!["(42, true)".to_string()];

        let mut abi = ABIParser::new();

        let function_name = "takes_tuple";

        let encoded = abi.encode_with_function_selector(json_abi, function_name, &values)?;

        let expected_encode = "000000001cc7bb2c000000000000002a0000000000000001";
        assert_eq!(encoded, expected_encode);
        Ok(())
    }

    #[test]
    fn nested_tuple_encode_and_decode() -> Result<(), Error> {
        let json_abi = r#"
        {
          "types": [
            {
              "typeId": 0,
              "type": "()",
              "components": [],
              "typeParameters": null
            },
            {
              "typeId": 1,
              "type": "(_, _)",
              "components": [
                {
                  "name": "__tuple_element",
                  "type": 7,
                  "typeArguments": null
                },
                {
                  "name": "__tuple_element",
                  "type": 3,
                  "typeArguments": null
                }
              ],
              "typeParameters": null
            },
            {
              "typeId": 2,
              "type": "(_, _, _)",
              "components": [
                {
                  "name": "__tuple_element",
                  "type": 1,
                  "typeArguments": null
                },
                {
                  "name": "__tuple_element",
                  "type": 6,
                  "typeArguments": null
                },
                {
                  "name": "__tuple_element",
                  "type": 4,
                  "typeArguments": null
                }
              ],
              "typeParameters": null
            },
            {
              "typeId": 3,
              "type": "bool",
              "components": null,
              "typeParameters": null
            },
            {
              "typeId": 4,
              "type": "enum State",
              "components": [
                {
                  "name": "A",
                  "type": 0,
                  "typeArguments": null
                },
                {
                  "name": "B",
                  "type": 0,
                  "typeArguments": null
                },
                {
                  "name": "C",
                  "type": 0,
                  "typeArguments": null
                }
              ],
              "typeParameters": null
            },
            {
              "typeId": 5,
              "type": "str[4]",
              "components": null,
              "typeParameters": null
            },
            {
              "typeId": 6,
              "type": "struct Person",
              "components": [
                {
                  "name": "name",
                  "type": 5,
                  "typeArguments": null
                }
              ],
              "typeParameters": null
            },
            {
              "typeId": 7,
              "type": "u64",
              "components": null,
              "typeParameters": null
            }
          ],
          "functions": [
            {
              "inputs": [
                {
                  "name": "input",
                  "type": 2,
                  "typeArguments": null
                }
              ],
              "name": "takes_nested_tuple",
              "output": {
                "name": "",
                "type": 0,
                "typeArguments": null
              }
            }
          ]
        }
        "#;

        let values: Vec<String> = vec!["((42, true), (John), (1, 0))".to_string()];

        let mut abi = ABIParser::new();

        let function_name = "takes_nested_tuple";

        let encoded = abi.encode_with_function_selector(json_abi, function_name, &values)?;

        println!("Function: {}", hex::encode(abi.fn_selector.unwrap()));
        let expected_encode =
            "00000000ebb8d011000000000000002a00000000000000014a6f686e000000000000000000000001";
        assert_eq!(encoded, expected_encode);
        Ok(())
    }

    // TODO: Move tests using the old abigen to the new one.
    // Currently, they will be skipped. Even though we're not fully testing these at
    // unit level, they're tested at integration level, in the main harness.rs file.

    // #[test]
    // fn fn_selector_single_primitive() -> Result<(), Error> {
    //     let p = Property {
    //         name: "foo".into(),
    //         type_field: "u64".into(),
    //         components: None,
    //     };
    //     let params = vec![p];
    //     let selector = build_fn_selector("my_func", &params)?;

    //     assert_eq!(selector, "my_func(u64)");
    //     Ok(())
    // }

    // #[test]
    // fn fn_selector_multiple_primitives() -> Result<(), Error> {
    //     let p1 = Property {
    //         name: "foo".into(),
    //         type_field: "u64".into(),
    //         components: None,
    //     };
    //     let p2 = Property {
    //         name: "bar".into(),
    //         type_field: "bool".into(),
    //         components: None,
    //     };
    //     let params = vec![p1, p2];
    //     let selector = build_fn_selector("my_func", &params)?;

    //     assert_eq!(selector, "my_func(u64,bool)");
    //     Ok(())
    // }

    // #[test]
    // fn fn_selector_custom_type() -> Result<(), Error> {
    //     let inner_foo = Property {
    //         name: "foo".into(),
    //         type_field: "bool".into(),
    //         components: None,
    //     };

    //     let inner_bar = Property {
    //         name: "bar".into(),
    //         type_field: "u64".into(),
    //         components: None,
    //     };

    //     let p_struct = Property {
    //         name: "my_struct".into(),
    //         type_field: "struct MyStruct".into(),
    //         components: Some(vec![inner_foo.clone(), inner_bar.clone()]),
    //     };

    //     let params = vec![p_struct];
    //     let selector = build_fn_selector("my_func", &params)?;

    //     assert_eq!(selector, "my_func(s(bool,u64))");

    //     let p_enum = Property {
    //         name: "my_enum".into(),
    //         type_field: "enum MyEnum".into(),
    //         components: Some(vec![inner_foo, inner_bar]),
    //     };
    //     let params = vec![p_enum];
    //     let selector = build_fn_selector("my_func", &params)?;

    //     assert_eq!(selector, "my_func(e(bool,u64))");
    //     Ok(())
    // }

    // #[test]
    // fn fn_selector_nested_struct() -> Result<(), Error> {
    //     let inner_foo = Property {
    //         name: "foo".into(),
    //         type_field: "bool".into(),
    //         components: None,
    //     };

    //     let inner_a = Property {
    //         name: "a".into(),
    //         type_field: "u64".into(),
    //         components: None,
    //     };

    //     let inner_b = Property {
    //         name: "b".into(),
    //         type_field: "u32".into(),
    //         components: None,
    //     };

    //     let inner_bar = Property {
    //         name: "bar".into(),
    //         type_field: "struct InnerStruct".into(),
    //         components: Some(vec![inner_a, inner_b]),
    //     };

    //     let p = Property {
    //         name: "my_struct".into(),
    //         type_field: "struct MyStruct".into(),
    //         components: Some(vec![inner_foo, inner_bar]),
    //     };

    //     let params = vec![p];
    //     let selector = build_fn_selector("my_func", &params)?;

    //     assert_eq!(selector, "my_func(s(bool,s(u64,u32)))");
    //     Ok(())
    // }

    // #[test]
    // fn fn_selector_nested_enum() -> Result<(), Error> {
    //     let inner_foo = Property {
    //         name: "foo".into(),
    //         type_field: "bool".into(),
    //         components: None,
    //     };

    //     let inner_a = Property {
    //         name: "a".into(),
    //         type_field: "u64".into(),
    //         components: None,
    //     };

    //     let inner_b = Property {
    //         name: "b".into(),
    //         type_field: "u32".into(),
    //         components: None,
    //     };

    //     let inner_bar = Property {
    //         name: "bar".into(),
    //         type_field: "enum InnerEnum".into(),
    //         components: Some(vec![inner_a, inner_b]),
    //     };

    //     let p = Property {
    //         name: "my_enum".into(),
    //         type_field: "enum MyEnum".into(),
    //         components: Some(vec![inner_foo, inner_bar]),
    //     };

    //     let params = vec![p];
    //     let selector = build_fn_selector("my_func", &params)?;

    //     assert_eq!(selector, "my_func(e(bool,e(u64,u32)))");
    //     Ok(())
    // }

    // #[test]
    // fn fn_selector_nested_custom_types() -> Result<(), Error> {
    //     let inner_foo = Property {
    //         name: "foo".into(),
    //         type_field: "bool".into(),
    //         components: None,
    //     };

    //     let inner_a = Property {
    //         name: "a".into(),
    //         type_field: "u64".into(),
    //         components: None,
    //     };

    //     let inner_b = Property {
    //         name: "b".into(),
    //         type_field: "u32".into(),
    //         components: None,
    //     };

    //     let mut inner_custom = Property {
    //         name: "bar".into(),
    //         type_field: "enum InnerEnum".into(),
    //         components: Some(vec![inner_a, inner_b]),
    //     };

    //     let p = Property {
    //         name: "my_struct".into(),
    //         type_field: "struct MyStruct".into(),
    //         components: Some(vec![inner_foo.clone(), inner_custom.clone()]),
    //     };

    //     let params = vec![p];
    //     let selector = build_fn_selector("my_func", &params)?;

    //     assert_eq!(selector, "my_func(s(bool,e(u64,u32)))");

    //     inner_custom.type_field = "struct InnerStruct".to_string();
    //     let p = Property {
    //         name: "my_enum".into(),
    //         type_field: "enum MyEnum".into(),
    //         components: Some(vec![inner_foo, inner_custom]),
    //     };
    //     let params = vec![p];
    //     let selector = build_fn_selector("my_func", &params)?;
    //     assert_eq!(selector, "my_func(e(bool,s(u64,u32)))");
    //     Ok(())
    // }

    #[test]
    fn strings_must_have_correct_length() {
        let json_abi = r#"
        {
          "types": [
            {
              "typeId": 0,
              "type": "()",
              "components": [],
              "typeParameters": null
            },
            {
              "typeId": 1,
              "type": "str[4]",
              "components": null,
              "typeParameters": null
            }
          ],
          "functions": [
            {
              "inputs": [
                {
                  "name": "foo",
                  "type": 1,
                  "typeArguments": null
                }
              ],
              "name": "takes_string",
              "output": {
                "name": "",
                "type": 0,
                "typeArguments": null
              }
            }
          ]
        }
        "#;

        let values: Vec<String> = vec!["fue".to_string()];
        let mut abi = ABIParser::new();
        let function_name = "takes_string";
        let error_message = abi
            .encode(json_abi, function_name, &values)
            .unwrap_err()
            .to_string();

        assert!(error_message.contains("String data has len "));
    }

    #[test]
    fn strings_must_have_correct_length_custom_types() {
        let json_abi = r#"
        {
          "types": [
            {
              "typeId": 0,
              "type": "()",
              "components": [],
              "typeParameters": null
            },
            {
              "typeId": 1,
              "type": "[_; 2]",
              "components": [
                {
                  "name": "__array_element",
                  "type": 4,
                  "typeArguments": null
                }
              ],
              "typeParameters": null
            },
            {
              "typeId": 2,
              "type": "str[4]",
              "components": null,
              "typeParameters": null
            },
            {
              "typeId": 3,
              "type": "struct MyStruct",
              "components": [
                {
                  "name": "foo",
                  "type": 1,
                  "typeArguments": null
                },
                {
                  "name": "bar",
                  "type": 2,
                  "typeArguments": null
                }
              ],
              "typeParameters": null
            },
            {
              "typeId": 4,
              "type": "u8",
              "components": null,
              "typeParameters": null
            }
          ],
          "functions": [
            {
              "inputs": [
                {
                  "name": "value",
                  "type": 3,
                  "typeArguments": null
                }
              ],
              "name": "takes_struct",
              "output": {
                "name": "",
                "type": 0,
                "typeArguments": null
              }
            }
          ]
        }
        "#;

        let values: Vec<String> = vec!["([0, 0], fuell)".to_string()];
        let mut abi = ABIParser::new();
        let function_name = "takes_struct";
        let error_message = abi
            .encode(json_abi, function_name, &values)
            .unwrap_err()
            .to_string();

        assert!(error_message.contains("String data has len "));
    }

    #[test]
    fn value_string_must_have_all_ascii_chars() {
        let json_abi = r#"
        {
          "types": [
            {
              "typeId": 0,
              "type": "()",
              "components": [],
              "typeParameters": null
            },
            {
              "typeId": 1,
              "type": "str[4]",
              "components": null,
              "typeParameters": null
            },
            {
              "typeId": 2,
              "type": "struct MyEnum",
              "components": [
                {
                  "name": "foo",
                  "type": 3,
                  "typeArguments": null
                },
                {
                  "name": "bar",
                  "type": 1,
                  "typeArguments": null
                }
              ],
              "typeParameters": null
            },
            {
              "typeId": 3,
              "type": "u32",
              "components": null,
              "typeParameters": null
            }
          ],
          "functions": [
            {
              "inputs": [
                {
                  "name": "my_enum",
                  "type": 2,
                  "typeArguments": null
                }
              ],
              "name": "takes_enum",
              "output": {
                "name": "",
                "type": 0,
                "typeArguments": null
              }
            }
          ]
        }
        "#;

        let values: Vec<String> = vec!["(0, fueŁ)".to_string()];
        let mut abi = ABIParser::new();
        let function_name = "takes_enum";
        let error_message = abi
            .encode(json_abi, function_name, &values)
            .unwrap_err()
            .to_string();

        assert_eq!(
            "Invalid data: value string can only contain ascii characters",
            error_message
        );
    }
}
