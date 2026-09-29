//! Discovery-to-emission coverage for Helm's `global` namespace at every
//! chart values root (the analyzed chart and each dependency instance under
//! its alias), against the Helm v4.2.3 matrix in
//! `round8-f1-evidence/matrix`.

use color_eyre::eyre;
use indoc::indoc;
use serde_json::json;
use vfs::VfsPath;

use test_util::prelude::sim_assert_eq;

const CONSTANT_CONFIGMAP: &str = indoc! {"
    apiVersion: v1
    kind: ConfigMap
    metadata:
      name: constant
    data:
      key: value
"};

fn chart(dir: &VfsPath, name: &str, dependencies: &str) -> eyre::Result<()> {
    test_util::write(
        &dir.join("Chart.yaml")?,
        format!("apiVersion: v2\nname: {name}\nversion: 0.1.0\n{dependencies}"),
    )?;
    test_util::write(&dir.join("values.yaml")?, "{}\n")?;
    test_util::write(&dir.join("templates/configmap.yaml")?, CONSTANT_CONFIGMAP)?;
    Ok(())
}

/// The matrix chart tree: `parent` depends on `child` twice, aliased `kid`
/// and `pup`, and `child` depends on `grandchild`. With `consumer`, the
/// grandchild renders `global.imageRegistry` through a strict string call.
fn witness_parent(consumer: bool) -> eyre::Result<VfsPath> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    chart(
        &root,
        "parent",
        indoc! {"
            dependencies:
              - name: child
                version: 0.1.0
                alias: kid
              - name: child
                version: 0.1.0
                alias: pup
        "},
    )?;
    let child = root.join("charts/child")?;
    chart(
        &child,
        "child",
        indoc! {"
            dependencies:
              - name: grandchild
                version: 0.1.0
        "},
    )?;
    let grandchild = child.join("charts/grandchild")?;
    chart(&grandchild, "grandchild", "")?;
    if consumer {
        test_util::write(
            &grandchild.join("templates/registry.yaml")?,
            indoc! {r#"
                apiVersion: v1
                kind: ConfigMap
                metadata:
                  name: registry
                data:
                  registry: {{ .Values.global.imageRegistry | default "none" | trunc 63 | quote }}
            "#},
        )?;
    }
    Ok(root)
}

fn generated_schema(chart_dir: VfsPath) -> eyre::Result<serde_json::Value> {
    let session = crate::AnalysisSession::new(crate::GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: crate::generation::SchemaProfile::default().into(),
        authoring: crate::generation::AuthoringPolicy::default(),
        provider: crate::provider::ProviderOptions {
            disable_k8s_schemas: true,
            allow_net: false,
            ..Default::default()
        },
    });
    Ok(session.generated_schema()?.schema)
}

/// Every discovered chart instance (the root, both aliases, and the nested
/// grandchild under each) reserves `global`, including instances whose
/// defaults are empty.
#[test]
fn every_discovered_chart_instance_reserves_global() -> eyre::Result<()> {
    let schema = generated_schema(witness_parent(false)?)?;
    sim_assert_eq!(have: schema, want: json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "global": {},
            "kid": {
                "additionalProperties": {},
                "properties": {
                    "global": {},
                    "grandchild": {
                        "additionalProperties": {},
                        "properties": {
                            "global": {}
                        },
                        "type": "object"
                    }
                },
                "type": "object"
            },
            "pup": {
                "additionalProperties": {},
                "properties": {
                    "global": {},
                    "grandchild": {
                        "additionalProperties": {},
                        "properties": {
                            "global": {}
                        },
                        "type": "object"
                    }
                },
                "type": "object"
            }
        },
        "type": "object"
    }));
    Ok(())
}

/// The witness tree's full schema, and the coalesced documents Helm v4.2.3
/// hands it (`round8-f1-evidence/matrix/witness-tree/coalesced.txt`)
/// against that schema: every render is accepted and every abort rejected.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn a_nested_global_consumer_follows_the_helm_matrix() -> eyre::Result<()> {
    let schema = generated_schema(witness_parent(true)?)?;
    sim_assert_eq!(have: &schema, want: &json!({
        "$defs": {
            "t": {
                "anyOf": [
                    {
                        "const": true
                    },
                    {
                        "not": {
                            "const": 0
                        },
                        "type": "number"
                    },
                    {
                        "minLength": 1,
                        "type": "string"
                    },
                    {
                        "minItems": 1,
                        "type": "array"
                    },
                    {
                        "minProperties": 1,
                        "type": "object"
                    }
                ]
            }
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": {
                    "allOf": [
                        {
                            "properties": {
                                "global": {
                                    "properties": {
                                        "imageRegistry": {
                                            "$ref": "#/$defs/t"
                                        }
                                    },
                                    "required": [
                                        "imageRegistry"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "global"
                            ],
                            "type": "object"
                        },
                        {
                            "properties": {
                                "global": {
                                    "properties": {
                                        "imageRegistry": {
                                            "not": {
                                                "enum": [
                                                    null
                                                ]
                                            }
                                        }
                                    },
                                    "required": [
                                        "imageRegistry"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "global"
                            ],
                            "type": "object"
                        },
                        {
                            "properties": {
                                "global": {
                                    "required": [
                                        "imageRegistry"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "global"
                            ],
                            "type": "object"
                        }
                    ]
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "global": {
                            "additionalProperties": {},
                            "properties": {
                                "imageRegistry": {
                                    "type": [
                                        "null",
                                        "string"
                                    ]
                                }
                            }
                        }
                    }
                }
            },
            {
                "if": {
                    "allOf": [
                        {
                            "properties": {
                                "kid": {
                                    "properties": {
                                        "global": {
                                            "properties": {
                                                "imageRegistry": {
                                                    "$ref": "#/$defs/t"
                                                }
                                            },
                                            "required": [
                                                "imageRegistry"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "global"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "kid"
                            ],
                            "type": "object"
                        },
                        {
                            "properties": {
                                "kid": {
                                    "properties": {
                                        "global": {
                                            "properties": {
                                                "imageRegistry": {
                                                    "not": {
                                                        "enum": [
                                                            null
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "imageRegistry"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "global"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "kid"
                            ],
                            "type": "object"
                        },
                        {
                            "properties": {
                                "kid": {
                                    "properties": {
                                        "global": {
                                            "required": [
                                                "imageRegistry"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "global"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "kid"
                            ],
                            "type": "object"
                        },
                        {
                            "anyOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "global": {
                                                        "properties": {
                                                            "imageRegistry": {}
                                                        },
                                                        "required": [
                                                            "imageRegistry"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "global"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "global": {
                                                    "properties": {
                                                        "imageRegistry": {
                                                            "enum": [
                                                                null
                                                            ]
                                                        }
                                                    },
                                                    "required": [
                                                        "imageRegistry"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "global"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "not": {
                                        "properties": {
                                            "global": {
                                                "required": [
                                                    "imageRegistry"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        "required": [
                                            "global"
                                        ],
                                        "type": "object"
                                    }
                                }
                            ]
                        }
                    ]
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "kid": {
                            "additionalProperties": {},
                            "properties": {
                                "global": {
                                    "additionalProperties": {},
                                    "properties": {
                                        "imageRegistry": {
                                            "type": [
                                                "null",
                                                "string"
                                            ]
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            {
                "if": {
                    "allOf": [
                        {
                            "properties": {
                                "kid": {
                                    "properties": {
                                        "grandchild": {
                                            "properties": {
                                                "global": {
                                                    "properties": {
                                                        "imageRegistry": {
                                                            "$ref": "#/$defs/t"
                                                        }
                                                    },
                                                    "required": [
                                                        "imageRegistry"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "global"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "grandchild"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "kid"
                            ],
                            "type": "object"
                        },
                        {
                            "anyOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "global": {
                                                        "properties": {
                                                            "imageRegistry": {}
                                                        },
                                                        "required": [
                                                            "imageRegistry"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "global"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "global": {
                                                    "properties": {
                                                        "imageRegistry": {
                                                            "enum": [
                                                                null
                                                            ]
                                                        }
                                                    },
                                                    "required": [
                                                        "imageRegistry"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "global"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "not": {
                                        "properties": {
                                            "global": {
                                                "required": [
                                                    "imageRegistry"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        "required": [
                                            "global"
                                        ],
                                        "type": "object"
                                    }
                                }
                            ]
                        },
                        {
                            "anyOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kid": {
                                                        "properties": {
                                                            "global": {
                                                                "properties": {
                                                                    "imageRegistry": {}
                                                                },
                                                                "required": [
                                                                    "imageRegistry"
                                                                ],
                                                                "type": "object"
                                                            }
                                                        },
                                                        "required": [
                                                            "global"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "kid"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kid": {
                                                    "properties": {
                                                        "global": {
                                                            "properties": {
                                                                "imageRegistry": {
                                                                    "enum": [
                                                                        null
                                                                    ]
                                                                }
                                                            },
                                                            "required": [
                                                                "imageRegistry"
                                                            ],
                                                            "type": "object"
                                                        }
                                                    },
                                                    "required": [
                                                        "global"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "kid"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "not": {
                                        "properties": {
                                            "kid": {
                                                "properties": {
                                                    "global": {
                                                        "required": [
                                                            "imageRegistry"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "global"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        "required": [
                                            "kid"
                                        ],
                                        "type": "object"
                                    }
                                }
                            ]
                        }
                    ]
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "kid": {
                            "additionalProperties": {},
                            "properties": {
                                "grandchild": {
                                    "additionalProperties": {},
                                    "properties": {
                                        "global": {
                                            "additionalProperties": {},
                                            "properties": {
                                                "imageRegistry": {
                                                    "type": [
                                                        "null",
                                                        "string"
                                                    ]
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            {
                "if": {
                    "allOf": [
                        {
                            "properties": {
                                "pup": {
                                    "properties": {
                                        "global": {
                                            "properties": {
                                                "imageRegistry": {
                                                    "$ref": "#/$defs/t"
                                                }
                                            },
                                            "required": [
                                                "imageRegistry"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "global"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "pup"
                            ],
                            "type": "object"
                        },
                        {
                            "properties": {
                                "pup": {
                                    "properties": {
                                        "global": {
                                            "properties": {
                                                "imageRegistry": {
                                                    "not": {
                                                        "enum": [
                                                            null
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "imageRegistry"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "global"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "pup"
                            ],
                            "type": "object"
                        },
                        {
                            "properties": {
                                "pup": {
                                    "properties": {
                                        "global": {
                                            "required": [
                                                "imageRegistry"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "global"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "pup"
                            ],
                            "type": "object"
                        },
                        {
                            "anyOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "global": {
                                                        "properties": {
                                                            "imageRegistry": {}
                                                        },
                                                        "required": [
                                                            "imageRegistry"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "global"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "global": {
                                                    "properties": {
                                                        "imageRegistry": {
                                                            "enum": [
                                                                null
                                                            ]
                                                        }
                                                    },
                                                    "required": [
                                                        "imageRegistry"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "global"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "not": {
                                        "properties": {
                                            "global": {
                                                "required": [
                                                    "imageRegistry"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        "required": [
                                            "global"
                                        ],
                                        "type": "object"
                                    }
                                }
                            ]
                        }
                    ]
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "pup": {
                            "additionalProperties": {},
                            "properties": {
                                "global": {
                                    "additionalProperties": {},
                                    "properties": {
                                        "imageRegistry": {
                                            "type": [
                                                "null",
                                                "string"
                                            ]
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            {
                "if": {
                    "allOf": [
                        {
                            "properties": {
                                "pup": {
                                    "properties": {
                                        "grandchild": {
                                            "properties": {
                                                "global": {
                                                    "properties": {
                                                        "imageRegistry": {
                                                            "$ref": "#/$defs/t"
                                                        }
                                                    },
                                                    "required": [
                                                        "imageRegistry"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "global"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "grandchild"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "pup"
                            ],
                            "type": "object"
                        },
                        {
                            "anyOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "global": {
                                                        "properties": {
                                                            "imageRegistry": {}
                                                        },
                                                        "required": [
                                                            "imageRegistry"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "global"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "global": {
                                                    "properties": {
                                                        "imageRegistry": {
                                                            "enum": [
                                                                null
                                                            ]
                                                        }
                                                    },
                                                    "required": [
                                                        "imageRegistry"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "global"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "not": {
                                        "properties": {
                                            "global": {
                                                "required": [
                                                    "imageRegistry"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        "required": [
                                            "global"
                                        ],
                                        "type": "object"
                                    }
                                }
                            ]
                        },
                        {
                            "anyOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "pup": {
                                                        "properties": {
                                                            "global": {
                                                                "properties": {
                                                                    "imageRegistry": {}
                                                                },
                                                                "required": [
                                                                    "imageRegistry"
                                                                ],
                                                                "type": "object"
                                                            }
                                                        },
                                                        "required": [
                                                            "global"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "pup"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "pup": {
                                                    "properties": {
                                                        "global": {
                                                            "properties": {
                                                                "imageRegistry": {
                                                                    "enum": [
                                                                        null
                                                                    ]
                                                                }
                                                            },
                                                            "required": [
                                                                "imageRegistry"
                                                            ],
                                                            "type": "object"
                                                        }
                                                    },
                                                    "required": [
                                                        "global"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "pup"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "not": {
                                        "properties": {
                                            "pup": {
                                                "properties": {
                                                    "global": {
                                                        "required": [
                                                            "imageRegistry"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "global"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        "required": [
                                            "pup"
                                        ],
                                        "type": "object"
                                    }
                                }
                            ]
                        }
                    ]
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "pup": {
                            "additionalProperties": {},
                            "properties": {
                                "grandchild": {
                                    "additionalProperties": {},
                                    "properties": {
                                        "global": {
                                            "additionalProperties": {},
                                            "properties": {
                                                "imageRegistry": {
                                                    "type": [
                                                        "null",
                                                        "string"
                                                    ]
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            },
            {
                "if": {
                    "allOf": [
                        {
                            "properties": {
                                "kid": {
                                    "properties": {
                                        "grandchild": {
                                            "allOf": [
                                                {
                                                    "type": "object"
                                                },
                                                {
                                                    "anyOf": [
                                                        {
                                                            "not": {
                                                                "properties": {
                                                                    "global": {}
                                                                },
                                                                "required": [
                                                                    "global"
                                                                ],
                                                                "type": "object"
                                                            }
                                                        },
                                                        {
                                                            "properties": {
                                                                "global": {
                                                                    "enum": [
                                                                        null
                                                                    ]
                                                                }
                                                            },
                                                            "required": [
                                                                "global"
                                                            ],
                                                            "type": "object"
                                                        }
                                                    ]
                                                }
                                            ]
                                        }
                                    },
                                    "required": [
                                        "grandchild"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "kid"
                            ],
                            "type": "object"
                        },
                        {
                            "properties": {
                                "kid": {
                                    "allOf": [
                                        {
                                            "type": "object"
                                        },
                                        {
                                            "anyOf": [
                                                {
                                                    "not": {
                                                        "properties": {
                                                            "grandchild": {}
                                                        },
                                                        "required": [
                                                            "grandchild"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                {
                                                    "properties": {
                                                        "grandchild": {
                                                            "enum": [
                                                                null
                                                            ]
                                                        }
                                                    },
                                                    "required": [
                                                        "grandchild"
                                                    ],
                                                    "type": "object"
                                                }
                                            ]
                                        }
                                    ]
                                }
                            },
                            "required": [
                                "kid"
                            ],
                            "type": "object"
                        }
                    ]
                },
                "then": false
            },
            {
                "if": {
                    "allOf": [
                        {
                            "properties": {
                                "pup": {
                                    "properties": {
                                        "grandchild": {
                                            "allOf": [
                                                {
                                                    "type": "object"
                                                },
                                                {
                                                    "anyOf": [
                                                        {
                                                            "not": {
                                                                "properties": {
                                                                    "global": {}
                                                                },
                                                                "required": [
                                                                    "global"
                                                                ],
                                                                "type": "object"
                                                            }
                                                        },
                                                        {
                                                            "properties": {
                                                                "global": {
                                                                    "enum": [
                                                                        null
                                                                    ]
                                                                }
                                                            },
                                                            "required": [
                                                                "global"
                                                            ],
                                                            "type": "object"
                                                        }
                                                    ]
                                                }
                                            ]
                                        }
                                    },
                                    "required": [
                                        "grandchild"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "pup"
                            ],
                            "type": "object"
                        },
                        {
                            "properties": {
                                "pup": {
                                    "allOf": [
                                        {
                                            "type": "object"
                                        },
                                        {
                                            "anyOf": [
                                                {
                                                    "not": {
                                                        "properties": {
                                                            "grandchild": {}
                                                        },
                                                        "required": [
                                                            "grandchild"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                {
                                                    "properties": {
                                                        "grandchild": {
                                                            "enum": [
                                                                null
                                                            ]
                                                        }
                                                    },
                                                    "required": [
                                                        "grandchild"
                                                    ],
                                                    "type": "object"
                                                }
                                            ]
                                        }
                                    ]
                                }
                            },
                            "required": [
                                "pup"
                            ],
                            "type": "object"
                        }
                    ]
                },
                "then": false
            }
        ],
        "properties": {
            "global": {
                "additionalProperties": {},
                "properties": {
                    "imageRegistry": {}
                }
            },
            "kid": {
                "additionalProperties": {},
                "properties": {
                    "global": {
                        "additionalProperties": {},
                        "properties": {
                            "imageRegistry": {}
                        }
                    },
                    "grandchild": {
                        "additionalProperties": {},
                        "properties": {
                            "global": {
                                "additionalProperties": {},
                                "properties": {
                                    "imageRegistry": {}
                                },
                                "type": "object"
                            }
                        },
                        "type": "object"
                    }
                },
                "type": "object"
            },
            "pup": {
                "additionalProperties": {},
                "properties": {
                    "global": {
                        "additionalProperties": {},
                        "properties": {
                            "imageRegistry": {}
                        }
                    },
                    "grandchild": {
                        "additionalProperties": {},
                        "properties": {
                            "global": {
                                "additionalProperties": {},
                                "properties": {
                                    "imageRegistry": {}
                                },
                                "type": "object"
                            }
                        },
                        "type": "object"
                    }
                },
                "type": "object"
            }
        },
        "type": "object"
    }));
    let validator = jsonschema::validator_for(&schema)?;
    let injected = |global: serde_json::Value| {
        json!({
            "global": global.clone(),
            "grandchild": { "global": global }
        })
    };
    let cells = [
        (
            "no --set: every instance receives an empty global",
            json!({ "kid": injected(json!({})), "pup": injected(json!({})) }),
            true,
        ),
        (
            "--set global.imageRegistry=x renders",
            json!({
                "global": { "imageRegistry": "x" },
                "kid": injected(json!({ "imageRegistry": "x" })),
                "pup": injected(json!({ "imageRegistry": "x" })),
            }),
            true,
        ),
        (
            "--set global.imageRegistry=1 aborts in trunc",
            json!({
                "global": { "imageRegistry": 1 },
                "kid": injected(json!({ "imageRegistry": 1 })),
                "pup": injected(json!({ "imageRegistry": 1 })),
            }),
            false,
        ),
        (
            "a values-file object registry aborts in trunc",
            json!({
                "global": { "imageRegistry": { "a": "b" } },
                "kid": injected(json!({ "imageRegistry": { "a": "b" } })),
                "pup": injected(json!({ "imageRegistry": { "a": "b" } })),
            }),
            false,
        ),
        (
            "an unrelated shared member renders",
            json!({
                "global": { "imageRegistry": "x", "other": 1 },
                "kid": injected(json!({ "imageRegistry": "x", "other": 1 })),
                "pup": injected(json!({ "imageRegistry": "x", "other": 1 })),
            }),
            true,
        ),
        (
            "a null root global renders; the instances keep their own",
            json!({
                "global": null,
                "kid": injected(json!({})),
                "pup": injected(json!({})),
            }),
            true,
        ),
    ];
    let have = cells
        .iter()
        .map(|(label, document, _)| (*label, validator.is_valid(document)))
        .collect::<Vec<_>>();
    let want = cells
        .iter()
        .map(|(label, _, renders)| (*label, *renders))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: have, want: want);
    Ok(())
}

/// `parent` depends on `child` as `kid` under `condition: kidEnabled`
/// (declared `false`), and `child` depends on `gc` (declared `gc: {}`, whose
/// members `child` reads under a guard), `grandchild` aliased `al`, and `und`
/// (undeclared).
fn disabled_ancestor_tree() -> eyre::Result<VfsPath> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    chart(
        &root,
        "parent",
        indoc! {"
            dependencies:
              - name: child
                version: 0.1.0
                alias: kid
                condition: kidEnabled
        "},
    )?;
    test_util::write(&root.join("values.yaml")?, "kidEnabled: false\n")?;
    let child = root.join("charts/child")?;
    chart(
        &child,
        "child",
        indoc! {"
            dependencies:
              - name: gc
                version: 0.1.0
              - name: grandchild
                version: 0.1.0
                alias: al
              - name: und
                version: 0.1.0
        "},
    )?;
    test_util::write(&child.join("values.yaml")?, "gc: {}\n")?;
    // openebs' loki reads `.Values.minio.enabled` of its nested `minio`.
    test_util::write(
        &child.join("templates/gc.yaml")?,
        indoc! {r"
            {{- if .Values.gc.enabled }}
            apiVersion: v1
            kind: ConfigMap
            metadata:
              name: gc
            data:
              replicas: {{ .Values.gc.replicas | quote }}
            {{- end }}
        "},
    )?;
    for name in ["gc", "grandchild", "und"] {
        chart(&child.join(format!("charts/{name}"))?, name, "")?;
    }
    Ok(root)
}

/// Helm v4.2.3 type-asserts dependency roots twice: over every loaded chart
/// before pruning disabled ones, where a nested root is keyed by its chart
/// NAME and a declared default deletes a user null first, and again while
/// rendering the charts that stay active. A root below a pruned ancestor is
/// never asserted the second time, so its null survives into the final
/// document (`round8-f1-evidence/rework1/tree.sh`): with `kid` disabled,
/// `kid.gc: null` and `kid.al: null`/`5` render; `kid.gc: 5` and
/// `kid.und: null` abort in the first pass; `kid: null` aborts. With `kid`
/// active, `kid.al: 5` aborts while rendering.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn disabled_ancestor_preserves_declared_dependency_null() -> eyre::Result<()> {
    let schema = generated_schema(disabled_ancestor_tree()?)?;
    let validator = jsonschema::validator_for(&schema)?;
    let off = |kid: serde_json::Value| json!({ "kidEnabled": false, "kid": kid });
    let on = |kid: serde_json::Value| json!({ "kidEnabled": true, "kid": kid });
    let cells = [
        ("off: kid.gc null renders", off(json!({ "gc": null })), true),
        ("off: kid.gc {} renders", off(json!({ "gc": {} })), true),
        ("off: kid.gc 5 aborts", off(json!({ "gc": 5 })), false),
        ("off: kid.al null renders", off(json!({ "al": null })), true),
        ("off: kid.al 5 renders", off(json!({ "al": 5 })), true),
        (
            "off: kid.und null aborts",
            off(json!({ "und": null })),
            false,
        ),
        (
            "off: kid null aborts",
            json!({ "kidEnabled": false, "kid": null }),
            false,
        ),
        ("on: kid.gc 5 aborts", on(json!({ "gc": 5 })), false),
        ("on: kid.al 5 aborts", on(json!({ "al": 5 })), false),
        ("on: kid.und null aborts", on(json!({ "und": null })), false),
    ];
    let have = cells
        .iter()
        .map(|(label, document, _)| (*label, validator.is_valid(document)))
        .collect::<Vec<_>>();
    let want = cells
        .iter()
        .map(|(label, _, renders)| (*label, *renders))
        .collect::<Vec<_>>();
    sim_assert_eq!(have: have, want: want);
    sim_assert_eq!(have: schema, want: json!({
        "$defs": {
            "t": {
                "anyOf": [
                    {
                        "const": true
                    },
                    {
                        "not": {
                            "const": 0
                        },
                        "type": "number"
                    },
                    {
                        "minLength": 1,
                        "type": "string"
                    },
                    {
                        "minItems": 1,
                        "type": "array"
                    },
                    {
                        "minProperties": 1,
                        "type": "object"
                    }
                ]
            }
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": {
                    "anyOf": [
                        {
                            "properties": {
                                "kidEnabled": {
                                    "enum": [
                                        true
                                    ]
                                }
                            },
                            "required": [
                                "kidEnabled"
                            ],
                            "type": "object"
                        },
                        {
                            "allOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kidEnabled": {}
                                                },
                                                "required": [
                                                    "kidEnabled"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kidEnabled": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "kidEnabled"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kidEnabled": {}
                                                },
                                                "required": [
                                                    "kidEnabled"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kidEnabled": {
                                                    "not": {
                                                        "enum": [
                                                            true
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "kidEnabled"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "kid": {
                            "additionalProperties": {},
                            "properties": {
                                "al": {
                                    "type": "object"
                                }
                            }
                        }
                    }
                }
            },
            {
                "if": {
                    "anyOf": [
                        {
                            "properties": {
                                "kidEnabled": {
                                    "enum": [
                                        true
                                    ]
                                }
                            },
                            "required": [
                                "kidEnabled"
                            ],
                            "type": "object"
                        },
                        {
                            "allOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kidEnabled": {}
                                                },
                                                "required": [
                                                    "kidEnabled"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kidEnabled": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "kidEnabled"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kidEnabled": {}
                                                },
                                                "required": [
                                                    "kidEnabled"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kidEnabled": {
                                                    "not": {
                                                        "enum": [
                                                            true
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "kidEnabled"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        },
                        {
                            "anyOf": [
                                {
                                    "properties": {
                                        "kidEnabled": {
                                            "$ref": "#/$defs/t"
                                        }
                                    },
                                    "required": [
                                        "kidEnabled"
                                    ],
                                    "type": "object"
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kidEnabled": {}
                                                },
                                                "required": [
                                                    "kidEnabled"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kidEnabled": {
                                                    "enum": [
                                                        null
                                                    ]
                                                }
                                            },
                                            "required": [
                                                "kidEnabled"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "kid": {
                            "additionalProperties": {},
                            "properties": {
                                "gc": {
                                    "type": "object"
                                }
                            }
                        }
                    }
                }
            },
            {
                "additionalProperties": {},
                "properties": {
                    "kid": {
                        "additionalProperties": {},
                        "properties": {
                            "gc": {
                                "type": [
                                    "null",
                                    "object"
                                ]
                            }
                        }
                    }
                }
            }
        ],
        "properties": {
            "global": {},
            "kid": {
                "additionalProperties": {},
                "properties": {
                    "al": {},
                    "gc": {
                        "additionalProperties": {},
                        "properties": {
                            "enabled": {},
                            "global": {},
                            "replicas": {}
                        }
                    },
                    "global": {},
                    "und": {
                        "additionalProperties": {},
                        "properties": {
                            "global": {}
                        },
                        "type": "object"
                    }
                },
                "type": "object"
            },
            "kidEnabled": {
                "type": "boolean"
            }
        },
        "type": "object"
    }));
    Ok(())
}

/// `parent` declares `kid: null` for its `child` dependency aliased `kid`,
/// optionally under `condition: kidEnabled` (declared `false`).
fn declared_null_dependency_tree(condition: bool) -> eyre::Result<VfsPath> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    let (dependencies, values) = if condition {
        (
            indoc! {"
                dependencies:
                  - name: child
                    version: 0.1.0
                    alias: kid
                    condition: kidEnabled
            "},
            indoc! {"
                kidEnabled: false
                kid: null
            "},
        )
    } else {
        (
            indoc! {"
                dependencies:
                  - name: child
                    version: 0.1.0
                    alias: kid
            "},
            "kid: null\n",
        )
    };
    chart(&root, "parent", dependencies)?;
    test_util::write(&root.join("values.yaml")?, values)?;
    chart(&root.join("charts/child")?, "child", "")?;
    Ok(root)
}

/// Helm v4.2.3 merges every active chart's own defaults with its active
/// dependencies' tables after pruning, so a parent default that is not a
/// table at an active dependency's key aborts every render ("type mismatch
/// on kid", matrix cells r4/r6), whatever the user supplies; with the
/// dependency disabled it renders (r5).
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn a_declared_non_table_dependency_default_aborts_while_the_dependency_is_active()
-> eyre::Result<()> {
    let always = generated_schema(declared_null_dependency_tree(false)?)?;
    let conditional = generated_schema(declared_null_dependency_tree(true)?)?;
    let verdicts = [
        (&always, json!({})),
        (&always, json!({ "kid": { "x": 1 } })),
        (&conditional, json!({ "kidEnabled": false })),
        (&conditional, json!({ "kidEnabled": true, "kid": {} })),
    ]
    .iter()
    .map(|(schema, document)| Ok(jsonschema::validator_for(schema)?.is_valid(document)))
    .collect::<eyre::Result<Vec<_>>>()?;
    sim_assert_eq!(have: verdicts, want: vec![false, false, true, false]);
    sim_assert_eq!(have: always, want: json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            false
        ],
        "properties": {
            "global": {},
            "kid": {
                "anyOf": [
                    {
                        "const": null
                    },
                    {
                        "properties": {
                            "global": {}
                        },
                        "type": "object"
                    }
                ]
            }
        },
        "type": "object"
    }));
    sim_assert_eq!(have: conditional, want: json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "additionalProperties": {},
                "properties": {
                    "kid": {
                        "type": [
                            "null",
                            "object"
                        ]
                    }
                }
            },
            {
                "if": {
                    "anyOf": [
                        {
                            "properties": {
                                "kidEnabled": {
                                    "enum": [
                                        true
                                    ]
                                }
                            },
                            "required": [
                                "kidEnabled"
                            ],
                            "type": "object"
                        },
                        {
                            "allOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kidEnabled": {}
                                                },
                                                "required": [
                                                    "kidEnabled"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kidEnabled": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "kidEnabled"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kidEnabled": {}
                                                },
                                                "required": [
                                                    "kidEnabled"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kidEnabled": {
                                                    "not": {
                                                        "enum": [
                                                            true
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "kidEnabled"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                },
                "then": {
                    "if": {
                        "properties": {
                            "kid": {
                                "not": {
                                    "enum": [
                                        null
                                    ]
                                }
                            }
                        },
                        "required": [
                            "kid"
                        ],
                        "type": "object"
                    },
                    "then": {
                        "additionalProperties": {},
                        "properties": {
                            "kid": {
                                "type": "object"
                            }
                        }
                    }
                }
            },
            {
                "if": {
                    "anyOf": [
                        {
                            "properties": {
                                "kidEnabled": {
                                    "enum": [
                                        true
                                    ]
                                }
                            },
                            "required": [
                                "kidEnabled"
                            ],
                            "type": "object"
                        },
                        {
                            "allOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kidEnabled": {}
                                                },
                                                "required": [
                                                    "kidEnabled"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kidEnabled": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "kidEnabled"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kidEnabled": {}
                                                },
                                                "required": [
                                                    "kidEnabled"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kidEnabled": {
                                                    "not": {
                                                        "enum": [
                                                            true
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "kidEnabled"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                },
                "then": false
            }
        ],
        "properties": {
            "global": {},
            "kid": {
                "properties": {
                    "global": {}
                }
            },
            "kidEnabled": {
                "type": "boolean"
            }
        },
        "type": "object"
    }));
    Ok(())
}

/// A generated schema's verdicts on coalesced documents.
fn verdicts(
    schema: &serde_json::Value,
    documents: &[serde_json::Value],
) -> eyre::Result<Vec<bool>> {
    let validator = jsonschema::validator_for(schema)?;
    Ok(documents
        .iter()
        .map(|document| validator.is_valid(document))
        .collect())
}

/// `parent` depends on `child` as `kid`, and `child` on `crdchart` aliased
/// `crds` under `condition: installCRDs` and `tags: [install-crds]`, with
/// `installCRDs: false` and `crds: {}` declared (Datadog's operator).
fn disabled_aliased_dependency_tree() -> eyre::Result<VfsPath> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    chart(
        &root,
        "parent",
        indoc! {"
            dependencies:
              - name: child
                version: 0.1.0
                alias: kid
        "},
    )?;
    let child = root.join("charts/child")?;
    chart(
        &child,
        "child",
        indoc! {"
            dependencies:
              - name: crdchart
                version: 0.1.0
                alias: crds
                condition: installCRDs
                tags:
                  - install-crds
        "},
    )?;
    test_util::write(
        &child.join("values.yaml")?,
        indoc! {"
            installCRDs: false
            crds: {}
        "},
    )?;
    chart(&child.join("charts/crdchart")?, "crdchart", "")?;
    Ok(root)
}

/// Helm v4.2.3 asserts a disabled aliased dependency in neither pass: the
/// first keys it by chart name, the second skips pruned charts
/// (`round8-f1-evidence/rework2/helm-cells.tsv` r6): `kid.crds: 5` renders
/// while `installCRDs` is false and aborts once it is true.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn a_disabled_aliased_dependency_is_not_table_asserted() -> eyre::Result<()> {
    let schema = generated_schema(disabled_aliased_dependency_tree()?)?;
    let documents = [
        json!({ "kid": { "installCRDs": false, "crds": 5 } }),
        json!({ "kid": { "installCRDs": true, "crds": 5 } }),
        json!({ "kid": { "installCRDs": "x", "crds": 5 } }),
    ];
    sim_assert_eq!(have: verdicts(&schema, &documents)?, want: vec![true, false, false]);
    sim_assert_eq!(have: schema, want: json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "if": {
                    "anyOf": [
                        {
                            "properties": {
                                "kid": {
                                    "properties": {
                                        "installCRDs": {
                                            "enum": [
                                                true
                                            ]
                                        }
                                    },
                                    "required": [
                                        "installCRDs"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "kid"
                            ],
                            "type": "object"
                        },
                        {
                            "allOf": [
                                {
                                    "properties": {
                                        "tags": {
                                            "properties": {
                                                "install-crds": {
                                                    "enum": [
                                                        true
                                                    ]
                                                }
                                            },
                                            "required": [
                                                "install-crds"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "tags"
                                    ],
                                    "type": "object"
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kid": {
                                                        "properties": {
                                                            "installCRDs": {}
                                                        },
                                                        "required": [
                                                            "installCRDs"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "kid"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kid": {
                                                    "properties": {
                                                        "installCRDs": {
                                                            "not": {
                                                                "enum": [
                                                                    false
                                                                ]
                                                            }
                                                        }
                                                    },
                                                    "required": [
                                                        "installCRDs"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "kid"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kid": {
                                                        "properties": {
                                                            "installCRDs": {}
                                                        },
                                                        "required": [
                                                            "installCRDs"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "kid"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kid": {
                                                    "properties": {
                                                        "installCRDs": {
                                                            "not": {
                                                                "enum": [
                                                                    true
                                                                ]
                                                            }
                                                        }
                                                    },
                                                    "required": [
                                                        "installCRDs"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "kid"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        },
                        {
                            "allOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kid": {
                                                        "properties": {
                                                            "installCRDs": {}
                                                        },
                                                        "required": [
                                                            "installCRDs"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "kid"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kid": {
                                                    "properties": {
                                                        "installCRDs": {
                                                            "not": {
                                                                "enum": [
                                                                    false
                                                                ]
                                                            }
                                                        }
                                                    },
                                                    "required": [
                                                        "installCRDs"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "kid"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "kid": {
                                                        "properties": {
                                                            "installCRDs": {}
                                                        },
                                                        "required": [
                                                            "installCRDs"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "kid"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "kid": {
                                                    "properties": {
                                                        "installCRDs": {
                                                            "not": {
                                                                "enum": [
                                                                    true
                                                                ]
                                                            }
                                                        }
                                                    },
                                                    "required": [
                                                        "installCRDs"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "kid"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "tags": {
                                                        "properties": {
                                                            "install-crds": {}
                                                        },
                                                        "required": [
                                                            "install-crds"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "tags"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "tags": {
                                                    "properties": {
                                                        "install-crds": {
                                                            "not": {
                                                                "enum": [
                                                                    false
                                                                ]
                                                            }
                                                        }
                                                    },
                                                    "required": [
                                                        "install-crds"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "tags"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                },
                "then": {
                    "additionalProperties": {},
                    "properties": {
                        "kid": {
                            "additionalProperties": {},
                            "properties": {
                                "crds": {
                                    "type": "object"
                                }
                            }
                        }
                    }
                }
            }
        ],
        "properties": {
            "global": {},
            "kid": {
                "additionalProperties": {},
                "properties": {
                    "crds": {},
                    "global": {},
                    "installCRDs": {
                        "type": "boolean"
                    }
                },
                "type": "object"
            },
            "tags": {
                "additionalProperties": {},
                "properties": {
                    "install-crds": {
                        "type": "boolean"
                    }
                },
                "type": "object"
            }
        },
        "type": "object"
    }));
    Ok(())
}

/// A parent declaring `kid: null` for a vendored `charts/kid` its
/// `Chart.yaml` does not list, with `dependencies` omitted, empty, or naming
/// only a sibling that is pruned: Helm's dependency metadata ends nil, so it
/// never merges the parent's defaults with the dependency tables, and every
/// render succeeds with `kid: {global: {}}` (rework2 cells r7a-r7c).
fn unlisted_dependency_tree(dependencies: &str) -> eyre::Result<VfsPath> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    chart(&root, "parent", dependencies)?;
    test_util::write(
        &root.join("values.yaml")?,
        indoc! {"
            sibEnabled: false
            kid: null
        "},
    )?;
    chart(&root.join("charts/kid")?, "kid", "")?;
    chart(&root.join("charts/sib")?, "sib", "")?;
    Ok(root)
}

#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn an_unlisted_dependency_with_a_null_default_renders() -> eyre::Result<()> {
    let mut schemas = Vec::new();
    for dependencies in [
        "",
        "dependencies: []\n",
        indoc! {"
            dependencies:
              - name: sib
                version: 0.1.0
                condition: sibEnabled
        "},
    ] {
        let schema = generated_schema(unlisted_dependency_tree(dependencies)?)?;
        let documents = [
            json!({ "sibEnabled": false, "kid": { "global": {} } }),
            json!({ "sibEnabled": false, "kid": 5 }),
        ];
        sim_assert_eq!(have: verdicts(&schema, &documents)?, want: vec![true, false]);
        schemas.push(schema);
    }
    sim_assert_eq!(have: schemas, want: vec![
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "global": {},
                "kid": {
                    "anyOf": [
                        {
                            "const": null
                        },
                        {
                            "properties": {
                                "global": {}
                            },
                            "type": "object"
                        }
                    ]
                },
                "sib": {
                    "additionalProperties": {},
                    "properties": {
                        "global": {}
                    },
                    "type": "object"
                },
                "sibEnabled": {}
            },
            "type": "object"
        }),
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "global": {},
                "kid": {
                    "anyOf": [
                        {
                            "const": null
                        },
                        {
                            "properties": {
                                "global": {}
                            },
                            "type": "object"
                        }
                    ]
                },
                "sib": {
                    "additionalProperties": {},
                    "properties": {
                        "global": {}
                    },
                    "type": "object"
                },
                "sibEnabled": {}
            },
            "type": "object"
        }),
        json!({
            "$schema": "http://json-schema.org/draft-07/schema#",
            "additionalProperties": false,
            "properties": {
                "global": {},
                "kid": {
                    "anyOf": [
                        {
                            "const": null
                        },
                        {
                            "properties": {
                                "global": {}
                            },
                            "type": "object"
                        }
                    ]
                },
                "sib": {
                    "additionalProperties": {},
                    "properties": {
                        "global": {}
                    },
                    "type": "object"
                },
                "sibEnabled": {
                    "type": "boolean"
                }
            },
            "type": "object"
        }),
    ]);
    Ok(())
}

/// `parent` declares `kid: null` for its listed dependency `kid`, which is
/// enabled by `condition: first,second` or by `tags: [extra]`.
fn boolean_enablement_tree(dependency: &str, values: &str) -> eyre::Result<VfsPath> {
    let root = VfsPath::new(vfs::MemoryFS::new());
    chart(&root, "parent", dependency)?;
    test_util::write(&root.join("values.yaml")?, values)?;
    chart(&root.join("charts/child")?, "child", "")?;
    Ok(root)
}

/// Helm v4.2.3 enables a dependency by the first condition holding a
/// BOOLEAN, skipping non-boolean ones, then by its tags (rework2 cells
/// r8a-r8c): with `first: ignored, second: false` the listed `kid` is pruned
/// and its `null` default renders, and with `second: true` it aborts; a
/// `false` tag prunes it, a `true` or non-boolean one aborts.
#[test]
#[expect(
    clippy::too_many_lines,
    reason = "the complete fixture scenario is clearest as one contiguous test"
)]
fn dependency_enablement_follows_helm_boolean_conditions_and_tags() -> eyre::Result<()> {
    let conditions = generated_schema(boolean_enablement_tree(
        indoc! {"
            dependencies:
              - name: child
                version: 0.1.0
                alias: kid
                condition: first,second
        "},
        indoc! {"
            kid: null
            first: ignored
            second: false
        "},
    )?)?;
    let tags = generated_schema(boolean_enablement_tree(
        indoc! {"
            dependencies:
              - name: child
                version: 0.1.0
                alias: kid
                tags:
                  - extra
        "},
        indoc! {"
            kid: null
            tags:
              extra: false
        "},
    )?)?;
    sim_assert_eq!(
        have: (
            verdicts(
                &conditions,
                &[
                    json!({ "first": "ignored", "second": false }),
                    json!({ "first": "ignored", "second": true }),
                    json!({ "first": false, "second": true }),
                ],
            )?,
            verdicts(
                &tags,
                &[
                    json!({ "tags": { "extra": false } }),
                    json!({ "tags": { "extra": true } }),
                    json!({ "tags": { "extra": "yes" } }),
                ],
            )?,
        ),
        want: (vec![true, false, true], vec![true, false, false])
    );
    sim_assert_eq!(have: conditions, want: json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "additionalProperties": {},
                "properties": {
                    "kid": {
                        "type": [
                            "null",
                            "object"
                        ]
                    }
                }
            },
            {
                "if": {
                    "anyOf": [
                        {
                            "properties": {
                                "first": {
                                    "enum": [
                                        true
                                    ]
                                }
                            },
                            "required": [
                                "first"
                            ],
                            "type": "object"
                        },
                        {
                            "allOf": [
                                {
                                    "properties": {
                                        "second": {
                                            "enum": [
                                                true
                                            ]
                                        }
                                    },
                                    "required": [
                                        "second"
                                    ],
                                    "type": "object"
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "first": {}
                                                },
                                                "required": [
                                                    "first"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "first": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "first"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "first": {}
                                                },
                                                "required": [
                                                    "first"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "first": {
                                                    "not": {
                                                        "enum": [
                                                            true
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "first"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        },
                        {
                            "allOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "first": {}
                                                },
                                                "required": [
                                                    "first"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "first": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "first"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "first": {}
                                                },
                                                "required": [
                                                    "first"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "first": {
                                                    "not": {
                                                        "enum": [
                                                            true
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "first"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "second": {}
                                                },
                                                "required": [
                                                    "second"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "second": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "second"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "second": {}
                                                },
                                                "required": [
                                                    "second"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "second": {
                                                    "not": {
                                                        "enum": [
                                                            true
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "second"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                },
                "then": {
                    "if": {
                        "properties": {
                            "kid": {
                                "not": {
                                    "enum": [
                                        null
                                    ]
                                }
                            }
                        },
                        "required": [
                            "kid"
                        ],
                        "type": "object"
                    },
                    "then": {
                        "additionalProperties": {},
                        "properties": {
                            "kid": {
                                "type": "object"
                            }
                        }
                    }
                }
            },
            {
                "if": {
                    "anyOf": [
                        {
                            "properties": {
                                "first": {
                                    "enum": [
                                        true
                                    ]
                                }
                            },
                            "required": [
                                "first"
                            ],
                            "type": "object"
                        },
                        {
                            "allOf": [
                                {
                                    "properties": {
                                        "second": {
                                            "enum": [
                                                true
                                            ]
                                        }
                                    },
                                    "required": [
                                        "second"
                                    ],
                                    "type": "object"
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "first": {}
                                                },
                                                "required": [
                                                    "first"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "first": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "first"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "first": {}
                                                },
                                                "required": [
                                                    "first"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "first": {
                                                    "not": {
                                                        "enum": [
                                                            true
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "first"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        },
                        {
                            "allOf": [
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "first": {}
                                                },
                                                "required": [
                                                    "first"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "first": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "first"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "first": {}
                                                },
                                                "required": [
                                                    "first"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "first": {
                                                    "not": {
                                                        "enum": [
                                                            true
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "first"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "second": {}
                                                },
                                                "required": [
                                                    "second"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "second": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "second"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "second": {}
                                                },
                                                "required": [
                                                    "second"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "second": {
                                                    "not": {
                                                        "enum": [
                                                            true
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "second"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        }
                    ]
                },
                "then": false
            }
        ],
        "properties": {
            "first": {
                "anyOf": [
                    {
                        "type": "boolean"
                    },
                    {
                        "type": "string"
                    }
                ]
            },
            "global": {},
            "kid": {
                "properties": {
                    "global": {}
                }
            },
            "second": {
                "type": "boolean"
            }
        },
        "type": "object"
    }));
    sim_assert_eq!(have: tags, want: json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
            {
                "additionalProperties": {},
                "properties": {
                    "kid": {
                        "type": [
                            "null",
                            "object"
                        ]
                    }
                }
            },
            {
                "if": {
                    "anyOf": [
                        {
                            "properties": {
                                "tags": {
                                    "properties": {
                                        "extra": {
                                            "enum": [
                                                true
                                            ]
                                        }
                                    },
                                    "required": [
                                        "extra"
                                    ],
                                    "type": "object"
                                }
                            },
                            "required": [
                                "tags"
                            ],
                            "type": "object"
                        },
                        {
                            "anyOf": [
                                {
                                    "not": {
                                        "properties": {
                                            "tags": {
                                                "properties": {
                                                    "extra": {}
                                                },
                                                "required": [
                                                    "extra"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        "required": [
                                            "tags"
                                        ],
                                        "type": "object"
                                    }
                                },
                                {
                                    "properties": {
                                        "tags": {
                                            "properties": {
                                                "extra": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "extra"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "tags"
                                    ],
                                    "type": "object"
                                }
                            ]
                        }
                    ]
                },
                "then": {
                    "if": {
                        "properties": {
                            "kid": {
                                "not": {
                                    "enum": [
                                        null
                                    ]
                                }
                            }
                        },
                        "required": [
                            "kid"
                        ],
                        "type": "object"
                    },
                    "then": {
                        "additionalProperties": {},
                        "properties": {
                            "kid": {
                                "type": "object"
                            }
                        }
                    }
                }
            },
            {
                "if": {
                    "allOf": [
                        {
                            "anyOf": [
                                {
                                    "properties": {
                                        "tags": {
                                            "properties": {
                                                "extra": {
                                                    "enum": [
                                                        true
                                                    ]
                                                }
                                            },
                                            "required": [
                                                "extra"
                                            ],
                                            "type": "object"
                                        }
                                    },
                                    "required": [
                                        "tags"
                                    ],
                                    "type": "object"
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "tags": {
                                                        "properties": {
                                                            "extra": {}
                                                        },
                                                        "required": [
                                                            "extra"
                                                        ],
                                                        "type": "object"
                                                    }
                                                },
                                                "required": [
                                                    "tags"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "tags": {
                                                    "properties": {
                                                        "extra": {
                                                            "not": {
                                                                "enum": [
                                                                    false
                                                                ]
                                                            }
                                                        }
                                                    },
                                                    "required": [
                                                        "extra"
                                                    ],
                                                    "type": "object"
                                                }
                                            },
                                            "required": [
                                                "tags"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        },
                        {
                            "anyOf": [
                                {
                                    "not": {
                                        "properties": {
                                            "tags": {}
                                        },
                                        "required": [
                                            "tags"
                                        ],
                                        "type": "object"
                                    }
                                },
                                {
                                    "properties": {
                                        "tags": {
                                            "enum": [
                                                null
                                            ]
                                        }
                                    },
                                    "required": [
                                        "tags"
                                    ],
                                    "type": "object"
                                }
                            ]
                        }
                    ]
                },
                "then": false
            }
        ],
        "properties": {
            "global": {},
            "kid": {
                "properties": {
                    "global": {}
                }
            },
            "tags": {
                "additionalProperties": {},
                "allOf": [
                    {
                        "if": {
                            "anyOf": [
                                {
                                    "properties": {
                                        "extra": {
                                            "enum": [
                                                true
                                            ]
                                        }
                                    },
                                    "required": [
                                        "extra"
                                    ],
                                    "type": "object"
                                },
                                {
                                    "anyOf": [
                                        {
                                            "not": {
                                                "properties": {
                                                    "extra": {}
                                                },
                                                "required": [
                                                    "extra"
                                                ],
                                                "type": "object"
                                            }
                                        },
                                        {
                                            "properties": {
                                                "extra": {
                                                    "not": {
                                                        "enum": [
                                                            false
                                                        ]
                                                    }
                                                }
                                            },
                                            "required": [
                                                "extra"
                                            ],
                                            "type": "object"
                                        }
                                    ]
                                }
                            ]
                        },
                        "then": false
                    }
                ],
                "properties": {
                    "extra": {
                        "type": "boolean"
                    }
                },
                "type": "object"
            }
        },
        "type": "object"
    }));
    Ok(())
}
