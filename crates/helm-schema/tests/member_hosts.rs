//! Member evaluation obligations over coalesced values, independent of rendered YAML.

use color_eyre::eyre::{self, OptionExt as _};
use helm_schema::AnalysisSession;
use helm_schema::generation::{AuthoringPolicy, DeclaredTypes, GenerateOptions, SchemaProfile};
use helm_schema::output::{EmitRequest, OutputPipelineOptions, ReferencePolicy};
use helm_schema::provider::ProviderOptions;
use indoc::{formatdoc, indoc};
use serde_json::{Value, json};
use test_util::prelude::sim_assert_eq;
use vfs::VfsPath;

const DIRECT_ACCESS_AND_POST_ACCESS_DEFAULT_ENCODED: &str = indoc! {r#"
    [
      {
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
          {
            "if": {
              "anyOf": [
                {
                  "not": {
                    "properties": {
                      "a": {}
                    },
                    "required": [
                      "a"
                    ],
                    "type": "object"
                  }
                },
                {
                  "properties": {
                    "a": {
                      "enum": [
                        null
                      ]
                    }
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                }
              ]
            },
            "then": false
          },
          {
            "if": {
              "properties": {
                "a": {
                  "not": {
                    "enum": [
                      null
                    ]
                  }
                }
              },
              "required": [
                "a"
              ],
              "type": "object"
            },
            "then": {
              "additionalProperties": {},
              "properties": {
                "a": {
                  "type": "object"
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      },
      {
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
          {
            "if": {
              "anyOf": [
                {
                  "not": {
                    "properties": {
                      "a": {}
                    },
                    "required": [
                      "a"
                    ],
                    "type": "object"
                  }
                },
                {
                  "properties": {
                    "a": {
                      "enum": [
                        null
                      ]
                    }
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                }
              ]
            },
            "then": false
          },
          {
            "if": {
              "properties": {
                "a": {
                  "not": {
                    "enum": [
                      null
                    ]
                  }
                }
              },
              "required": [
                "a"
              ],
              "type": "object"
            },
            "then": {
              "additionalProperties": {},
              "properties": {
                "a": {
                  "type": "object"
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      }
    ]
"#};

const BOUND_AND_GROUPED_RECEIVERS_ENCODED: &str = indoc! {r##"
    [
      {
        "$defs": {
          "constrains/a+a=null": {
            "$ref": "#/$defs/constrains~1a+a=null@3"
          },
          "constrains/a+a=null@2": {
            "$ref": "#/$defs/constrains~1a+a=null@3"
          },
          "constrains/a+a=null@3": {
            "anyOf": [
              {
                "not": {
                  "properties": {
                    "a": {}
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                }
              },
              {
                "properties": {
                  "a": {
                    "enum": [
                      null
                    ]
                  }
                },
                "required": [
                  "a"
                ],
                "type": "object"
              }
            ]
          },
          "constrains/a+a=null@4": {
            "not": {
              "$ref": "#/$defs/constrains~1a+a=null"
            }
          },
          "constrains/a+a=null@5": {
            "not": {
              "$ref": "#/$defs/constrains~1a+a=null@2"
            }
          }
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
          {
            "if": {
              "$ref": "#/$defs/constrains~1a+a=null@4"
            },
            "then": {
              "if": {
                "properties": {
                  "a": {
                    "not": {
                      "enum": [
                        null
                      ]
                    }
                  }
                },
                "required": [
                  "a"
                ],
                "type": "object"
              },
              "then": {
                "additionalProperties": {},
                "properties": {
                  "a": {
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
                  "$ref": "#/$defs/constrains~1a+a=null@3"
                },
                {
                  "$ref": "#/$defs/constrains~1a+a=null@5"
                }
              ]
            },
            "then": false
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      },
      {
        "$defs": {
          "constrains/a+a=null": {
            "$ref": "#/$defs/constrains~1a+a=null@3"
          },
          "constrains/a+a=null@2": {
            "$ref": "#/$defs/constrains~1a+a=null@3"
          },
          "constrains/a+a=null@3": {
            "anyOf": [
              {
                "not": {
                  "properties": {
                    "a": {}
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                }
              },
              {
                "properties": {
                  "a": {
                    "enum": [
                      null
                    ]
                  }
                },
                "required": [
                  "a"
                ],
                "type": "object"
              }
            ]
          },
          "constrains/a+a=null@4": {
            "not": {
              "$ref": "#/$defs/constrains~1a+a=null"
            }
          },
          "constrains/a+a=null@5": {
            "not": {
              "$ref": "#/$defs/constrains~1a+a=null@2"
            }
          }
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
          {
            "if": {
              "$ref": "#/$defs/constrains~1a+a=null@4"
            },
            "then": {
              "if": {
                "properties": {
                  "a": {
                    "not": {
                      "enum": [
                        null
                      ]
                    }
                  }
                },
                "required": [
                  "a"
                ],
                "type": "object"
              },
              "then": {
                "additionalProperties": {},
                "properties": {
                  "a": {
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
                  "$ref": "#/$defs/constrains~1a+a=null@3"
                },
                {
                  "$ref": "#/$defs/constrains~1a+a=null@5"
                }
              ]
            },
            "then": false
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      }
    ]
"##};

const DICT_CONTEXT_HELPER_ENCODED: &str = indoc! {r#"
    {
      "$schema": "http://json-schema.org/draft-07/schema#",
      "additionalProperties": false,
      "allOf": [
        {
          "if": {
            "anyOf": [
              {
                "not": {
                  "properties": {
                    "a": {}
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                }
              },
              {
                "properties": {
                  "a": {
                    "enum": [
                      null
                    ]
                  }
                },
                "required": [
                  "a"
                ],
                "type": "object"
              }
            ]
          },
          "then": false
        },
        {
          "if": {
            "properties": {
              "a": {
                "not": {
                  "enum": [
                    null
                  ]
                }
              }
            },
            "required": [
              "a"
            ],
            "type": "object"
          },
          "then": {
            "additionalProperties": {},
            "properties": {
              "a": {
                "type": "object"
              }
            }
          }
        }
      ],
      "properties": {
        "a": {
          "additionalProperties": {},
          "properties": {
            "b": {}
          }
        },
        "global": {}
      },
      "type": "object",
      "x-helm-schema-generated": true,
      "x-helm-schema-policy": {
        "annotation-format-version": 1,
        "authoring": {
          "declared-types": "annotate",
          "root": "closed"
        },
        "modifiers": {
          "overrides": {
            "count": 0,
            "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
          },
          "reference-mode": "bundled"
        },
        "narrowing": [],
        "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
        "policy-vocabulary-version": 2,
        "requested-profile": "full",
        "resolved": {
          "kind-partitions": true,
          "local-conditionals": true,
          "root-anchored-conditionals": true,
          "terminal-clauses": true
        }
      }
    }
"#};

const COALESCE_AND_DEFAULT_DICT_ENCODED: &str = indoc! {r##"
    [
      {
        "$defs": {
          "constrains/a+a=null": {
            "$ref": "#/$defs/constrains~1a+a=null@3"
          },
          "constrains/a+a=null@2": {
            "$ref": "#/$defs/constrains~1a+a=null@3"
          },
          "constrains/a+a=null@3": {
            "not": {
              "anyOf": [
                {
                  "not": {
                    "properties": {
                      "a": {}
                    },
                    "required": [
                      "a"
                    ],
                    "type": "object"
                  }
                },
                {
                  "properties": {
                    "a": {
                      "enum": [
                        null
                      ]
                    }
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                }
              ]
            }
          },
          "t": {
            "anyOf": [
              {
                "minProperties": 1,
                "type": "object"
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
                "const": true
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
                    "a": {
                      "$ref": "#/$defs/t"
                    }
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                },
                {
                  "$ref": "#/$defs/constrains~1a+a=null@2"
                },
                {
                  "$ref": "#/$defs/constrains~1a+a=null"
                }
              ]
            },
            "then": {
              "additionalProperties": {},
              "properties": {
                "a": {
                  "type": "object"
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      },
      {
        "$defs": {
          "constrains/a+a=null": {
            "$ref": "#/$defs/constrains~1a+a=null@3"
          },
          "constrains/a+a=null@2": {
            "$ref": "#/$defs/constrains~1a+a=null@3"
          },
          "constrains/a+a=null@3": {
            "not": {
              "anyOf": [
                {
                  "not": {
                    "properties": {
                      "a": {}
                    },
                    "required": [
                      "a"
                    ],
                    "type": "object"
                  }
                },
                {
                  "properties": {
                    "a": {
                      "enum": [
                        null
                      ]
                    }
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                }
              ]
            }
          },
          "t": {
            "anyOf": [
              {
                "minProperties": 1,
                "type": "object"
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
                "const": true
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
                    "a": {
                      "$ref": "#/$defs/t"
                    }
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                },
                {
                  "$ref": "#/$defs/constrains~1a+a=null@2"
                },
                {
                  "$ref": "#/$defs/constrains~1a+a=null"
                }
              ]
            },
            "then": {
              "additionalProperties": {},
              "properties": {
                "a": {
                  "type": "object"
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      }
    ]
"##};

const WITH_IF_AND_NONEMPTY_GUARDS_ENCODED: &str = indoc! {r##"
    [
      {
        "$defs": {
          "constrains/a+a=null": {
            "$ref": "#/$defs/constrains~1a+a=null@3"
          },
          "constrains/a+a=null@2": {
            "$ref": "#/$defs/constrains~1a+a=null@3"
          },
          "constrains/a+a=null@3": {
            "not": {
              "anyOf": [
                {
                  "not": {
                    "properties": {
                      "a": {}
                    },
                    "required": [
                      "a"
                    ],
                    "type": "object"
                  }
                },
                {
                  "properties": {
                    "a": {
                      "enum": [
                        null
                      ]
                    }
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                }
              ]
            }
          },
          "t": {
            "anyOf": [
              {
                "minProperties": 1,
                "type": "object"
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
                "const": true
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
                    "a": {
                      "$ref": "#/$defs/t"
                    }
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                },
                {
                  "$ref": "#/$defs/constrains~1a+a=null@2"
                },
                {
                  "$ref": "#/$defs/constrains~1a+a=null"
                }
              ]
            },
            "then": {
              "additionalProperties": {},
              "properties": {
                "a": {
                  "type": "object"
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      },
      {
        "$defs": {
          "t": {
            "anyOf": [
              {
                "minProperties": 1,
                "type": "object"
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
                "const": true
              }
            ]
          }
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
          {
            "if": {
              "properties": {
                "a": {
                  "$ref": "#/$defs/t"
                }
              },
              "required": [
                "a"
              ],
              "type": "object"
            },
            "then": {
              "if": {
                "properties": {
                  "a": {
                    "not": {
                      "enum": [
                        null
                      ]
                    }
                  }
                },
                "required": [
                  "a"
                ],
                "type": "object"
              },
              "then": {
                "additionalProperties": {},
                "properties": {
                  "a": {
                    "type": "object"
                  }
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      },
      {
        "$defs": {
          "t": {
            "anyOf": [
              {
                "minProperties": 1,
                "type": "object"
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
                "const": true
              }
            ]
          }
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
          {
            "if": {
              "properties": {
                "a": {
                  "$ref": "#/$defs/t"
                }
              },
              "required": [
                "a"
              ],
              "type": "object"
            },
            "then": {
              "if": {
                "properties": {
                  "a": {
                    "not": {
                      "enum": [
                        null
                      ]
                    }
                  }
                },
                "required": [
                  "a"
                ],
                "type": "object"
              },
              "then": {
                "additionalProperties": {},
                "properties": {
                  "a": {
                    "type": "object"
                  }
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      },
      {
        "$defs": {
          "t": {
            "anyOf": [
              {
                "minProperties": 1,
                "type": "object"
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
                "const": true
              }
            ]
          }
        },
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
          {
            "if": {
              "properties": {
                "a": {
                  "$ref": "#/$defs/t"
                }
              },
              "required": [
                "a"
              ],
              "type": "object"
            },
            "then": {
              "if": {
                "properties": {
                  "a": {
                    "not": {
                      "enum": [
                        null
                      ]
                    }
                  }
                },
                "required": [
                  "a"
                ],
                "type": "object"
              },
              "then": {
                "additionalProperties": {},
                "properties": {
                  "a": {
                    "type": "object"
                  }
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      }
    ]
"##};

const INDEPENDENT_GATE_ENCODED: &str = indoc! {r##"
    [
      {
        "$defs": {
          "constrains/enabled:t": {
            "properties": {
              "enabled": {
                "$ref": "#/$defs/t"
              }
            },
            "required": [
              "enabled"
            ],
            "type": "object"
          },
          "t": {
            "anyOf": [
              {
                "minProperties": 1,
                "type": "object"
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
                "const": true
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
                  "anyOf": [
                    {
                      "not": {
                        "properties": {
                          "a": {}
                        },
                        "required": [
                          "a"
                        ],
                        "type": "object"
                      }
                    },
                    {
                      "properties": {
                        "a": {
                          "enum": [
                            null
                          ]
                        }
                      },
                      "required": [
                        "a"
                      ],
                      "type": "object"
                    }
                  ]
                },
                {
                  "$ref": "#/$defs/constrains~1enabled:t"
                }
              ]
            },
            "then": false
          },
          {
            "if": {
              "$ref": "#/$defs/constrains~1enabled:t"
            },
            "then": {
              "if": {
                "properties": {
                  "a": {
                    "not": {
                      "enum": [
                        null
                      ]
                    }
                  }
                },
                "required": [
                  "a"
                ],
                "type": "object"
              },
              "then": {
                "additionalProperties": {},
                "properties": {
                  "a": {
                    "type": "object"
                  }
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "enabled": {},
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      },
      {
        "$defs": {
          "constrains/enabled:t": {
            "properties": {
              "enabled": {
                "$ref": "#/$defs/t"
              }
            },
            "required": [
              "enabled"
            ],
            "type": "object"
          },
          "t": {
            "anyOf": [
              {
                "minProperties": 1,
                "type": "object"
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
                "const": true
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
                  "anyOf": [
                    {
                      "not": {
                        "properties": {
                          "a": {}
                        },
                        "required": [
                          "a"
                        ],
                        "type": "object"
                      }
                    },
                    {
                      "properties": {
                        "a": {
                          "enum": [
                            null
                          ]
                        }
                      },
                      "required": [
                        "a"
                      ],
                      "type": "object"
                    }
                  ]
                },
                {
                  "$ref": "#/$defs/constrains~1enabled:t"
                }
              ]
            },
            "then": false
          },
          {
            "if": {
              "$ref": "#/$defs/constrains~1enabled:t"
            },
            "then": {
              "if": {
                "properties": {
                  "a": {
                    "not": {
                      "enum": [
                        null
                      ]
                    }
                  }
                },
                "required": [
                  "a"
                ],
                "type": "object"
              },
              "then": {
                "additionalProperties": {},
                "properties": {
                  "a": {
                    "type": "object"
                  }
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "enabled": {},
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      }
    ]
"##};

const LITERAL_TPL_AND_HELPER_EQUIVALENCE_ENCODED: &str = indoc! {r#"
    [
      {
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
          {
            "if": {
              "anyOf": [
                {
                  "not": {
                    "properties": {
                      "a": {}
                    },
                    "required": [
                      "a"
                    ],
                    "type": "object"
                  }
                },
                {
                  "properties": {
                    "a": {
                      "enum": [
                        null
                      ]
                    }
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                }
              ]
            },
            "then": false
          },
          {
            "if": {
              "properties": {
                "a": {
                  "not": {
                    "enum": [
                      null
                    ]
                  }
                }
              },
              "required": [
                "a"
              ],
              "type": "object"
            },
            "then": {
              "additionalProperties": {},
              "properties": {
                "a": {
                  "type": "object"
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      },
      {
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "allOf": [
          {
            "if": {
              "anyOf": [
                {
                  "not": {
                    "properties": {
                      "a": {}
                    },
                    "required": [
                      "a"
                    ],
                    "type": "object"
                  }
                },
                {
                  "properties": {
                    "a": {
                      "enum": [
                        null
                      ]
                    }
                  },
                  "required": [
                    "a"
                  ],
                  "type": "object"
                }
              ]
            },
            "then": false
          },
          {
            "if": {
              "properties": {
                "a": {
                  "not": {
                    "enum": [
                      null
                    ]
                  }
                }
              },
              "required": [
                "a"
              ],
              "type": "object"
            },
            "then": {
              "additionalProperties": {},
              "properties": {
                "a": {
                  "type": "object"
                }
              }
            }
          }
        ],
        "properties": {
          "a": {
            "additionalProperties": {},
            "properties": {
              "b": {}
            }
          },
          "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
          "annotation-format-version": 1,
          "authoring": {
            "declared-types": "annotate",
            "root": "closed"
          },
          "modifiers": {
            "overrides": {
              "count": 0,
              "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
            },
            "reference-mode": "bundled"
          },
          "narrowing": [],
          "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
          "policy-vocabulary-version": 2,
          "requested-profile": "full",
          "resolved": {
            "kind-partitions": true,
            "local-conditionals": true,
            "root-anchored-conditionals": true,
            "terminal-clauses": true
          }
        }
      }
    ]
"#};

const ARGO_CD_API_VERSION_OVERRIDES_MONITORING_ENCODED: &str = indoc! {r#"
    {
      "$schema": "http://json-schema.org/draft-07/schema#",
      "additionalProperties": false,
      "allOf": [
        {
          "if": {
            "properties": {
              "apiVersionOverrides": {
                "not": {
                  "enum": [
                    null
                  ]
                }
              }
            },
            "required": [
              "apiVersionOverrides"
            ],
            "type": "object"
          },
          "then": {
            "additionalProperties": {},
            "properties": {
              "apiVersionOverrides": {
                "type": "object"
              }
            }
          }
        },
        {
          "if": {
            "anyOf": [
              {
                "not": {
                  "properties": {
                    "apiVersionOverrides": {}
                  },
                  "required": [
                    "apiVersionOverrides"
                  ],
                  "type": "object"
                }
              },
              {
                "properties": {
                  "apiVersionOverrides": {
                    "enum": [
                      null
                    ]
                  }
                },
                "required": [
                  "apiVersionOverrides"
                ],
                "type": "object"
              }
            ]
          },
          "then": false
        }
      ],
      "properties": {
        "apiVersionOverrides": {
          "additionalProperties": {},
          "properties": {
            "monitoring": {}
          }
        },
        "global": {}
      },
      "type": "object",
      "x-helm-schema-generated": true,
      "x-helm-schema-policy": {
        "annotation-format-version": 1,
        "authoring": {
          "declared-types": "annotate",
          "root": "closed"
        },
        "modifiers": {
          "overrides": {
            "count": 0,
            "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
          },
          "reference-mode": "bundled"
        },
        "narrowing": [],
        "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
        "policy-vocabulary-version": 2,
        "requested-profile": "full",
        "resolved": {
          "kind-partitions": true,
          "local-conditionals": true,
          "root-anchored-conditionals": true,
          "terminal-clauses": true
        }
      }
    }
"#};

const HAS_KEY_PROTECTS_ABSENCE_ONLY_ENCODED: &str = indoc! {r#"
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "additionalProperties": false,
  "allOf": [
    {
      "if": {
        "required": [
          "a"
        ],
        "type": "object"
      },
      "then": {
        "additionalProperties": {},
        "properties": {
          "a": {
            "type": "object"
          }
        }
      }
    }
  ],
  "properties": {
    "a": {
      "additionalProperties": {},
      "properties": {
        "b": {}
      }
    },
    "global": {}
  },
  "type": "object",
  "x-helm-schema-generated": true,
  "x-helm-schema-policy": {
    "annotation-format-version": 1,
    "authoring": {
      "declared-types": "annotate",
      "root": "closed"
    },
    "modifiers": {
      "overrides": {
        "count": 0,
        "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
      },
      "reference-mode": "bundled"
    },
    "narrowing": [],
    "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
    "policy-vocabulary-version": 2,
    "requested-profile": "full",
    "resolved": {
      "kind-partitions": true,
      "local-conditionals": true,
      "root-anchored-conditionals": true,
      "terminal-clauses": true
    }
  }
}
"#};

const CILIUM_INDEX_IN_AND_ENCODED: &str = indoc! {r#"
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "additionalProperties": false,
  "allOf": [
    {
      "if": {
        "properties": {
          "extraConfig": {
            "not": {
              "enum": [
                null
              ]
            }
          }
        },
        "required": [
          "extraConfig"
        ],
        "type": "object"
      },
      "then": {
        "additionalProperties": {},
        "properties": {
          "extraConfig": {
            "type": "object"
          }
        }
      }
    },
    {
      "if": {
        "anyOf": [
          {
            "properties": {
              "extraConfig": {
                "enum": [
                  null
                ]
              }
            },
            "required": [
              "extraConfig"
            ],
            "type": "object"
          },
          {
            "not": {
              "properties": {
                "extraConfig": {}
              },
              "required": [
                "extraConfig"
              ],
              "type": "object"
            }
          }
        ]
      },
      "then": false
    }
  ],
  "properties": {
    "enabled": {},
    "extraConfig": {
      "additionalProperties": {},
      "properties": {
        "bpf-lb-mode": {}
      }
    },
    "global": {}
  },
  "type": "object",
  "x-helm-schema-generated": true,
  "x-helm-schema-policy": {
    "annotation-format-version": 1,
    "authoring": {
      "declared-types": "annotate",
      "root": "closed"
    },
    "modifiers": {
      "overrides": {
        "count": 0,
        "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
      },
      "reference-mode": "bundled"
    },
    "narrowing": [],
    "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
    "policy-vocabulary-version": 2,
    "requested-profile": "full",
    "resolved": {
      "kind-partitions": true,
      "local-conditionals": true,
      "root-anchored-conditionals": true,
      "terminal-clauses": true
    }
  }
}
"#};

const REDIS_AUTH_ACL_ENABLED_ENCODED: &str = indoc! {r#"
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "additionalProperties": false,
  "allOf": [
    {
      "if": {
        "anyOf": [
          {
            "properties": {
              "auth": {
                "properties": {
                  "acl": {
                    "enum": [
                      null
                    ]
                  }
                },
                "required": [
                  "acl"
                ],
                "type": "object"
              }
            },
            "required": [
              "auth"
            ],
            "type": "object"
          },
          {
            "not": {
              "properties": {
                "auth": {
                  "properties": {
                    "acl": {}
                  },
                  "required": [
                    "acl"
                  ],
                  "type": "object"
                }
              },
              "required": [
                "auth"
              ],
              "type": "object"
            }
          }
        ]
      },
      "then": false
    },
    {
      "if": {
        "properties": {
          "auth": {
            "not": {
              "enum": [
                null
              ]
            }
          }
        },
        "required": [
          "auth"
        ],
        "type": "object"
      },
      "then": {
        "additionalProperties": {},
        "properties": {
          "auth": {
            "type": "object"
          }
        }
      }
    },
    {
      "if": {
        "anyOf": [
          {
            "not": {
              "properties": {
                "auth": {}
              },
              "required": [
                "auth"
              ],
              "type": "object"
            }
          },
          {
            "properties": {
              "auth": {
                "enum": [
                  null
                ]
              }
            },
            "required": [
              "auth"
            ],
            "type": "object"
          }
        ]
      },
      "then": false
    }
  ],
  "properties": {
    "auth": {
      "additionalProperties": {},
      "properties": {
        "acl": {
          "additionalProperties": {},
          "properties": {
            "enabled": {}
          },
          "type": "object"
        }
      }
    },
    "global": {}
  },
  "type": "object",
  "x-helm-schema-generated": true,
  "x-helm-schema-policy": {
    "annotation-format-version": 1,
    "authoring": {
      "declared-types": "annotate",
      "root": "closed"
    },
    "modifiers": {
      "overrides": {
        "count": 0,
        "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
      },
      "reference-mode": "bundled"
    },
    "narrowing": [],
    "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
    "policy-vocabulary-version": 2,
    "requested-profile": "full",
    "resolved": {
      "kind-partitions": true,
      "local-conditionals": true,
      "root-anchored-conditionals": true,
      "terminal-clauses": true
    }
  }
}
"#};

const MILVUS_NESTED_INCLUDE_ENCODED: &str = indoc! {r#"
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "additionalProperties": false,
  "allOf": [
    {
      "if": {
        "anyOf": [
          {
            "not": {
              "properties": {
                "dataCoordinator": {}
              },
              "required": [
                "dataCoordinator"
              ],
              "type": "object"
            }
          },
          {
            "properties": {
              "dataCoordinator": {
                "enum": [
                  null
                ]
              }
            },
            "required": [
              "dataCoordinator"
            ],
            "type": "object"
          }
        ]
      },
      "then": false
    },
    {
      "if": {
        "anyOf": [
          {
            "not": {
              "properties": {
                "dataCoordinator": {
                  "properties": {
                    "service": {}
                  },
                  "required": [
                    "service"
                  ],
                  "type": "object"
                }
              },
              "required": [
                "dataCoordinator"
              ],
              "type": "object"
            }
          },
          {
            "properties": {
              "dataCoordinator": {
                "properties": {
                  "service": {
                    "enum": [
                      null
                    ]
                  }
                },
                "required": [
                  "service"
                ],
                "type": "object"
              }
            },
            "required": [
              "dataCoordinator"
            ],
            "type": "object"
          }
        ]
      },
      "then": false
    },
    {
      "if": {
        "properties": {
          "dataCoordinator": {
            "not": {
              "enum": [
                null
              ]
            }
          }
        },
        "required": [
          "dataCoordinator"
        ],
        "type": "object"
      },
      "then": {
        "additionalProperties": {},
        "properties": {
          "dataCoordinator": {
            "type": "object"
          }
        }
      }
    }
  ],
  "properties": {
    "dataCoordinator": {
      "additionalProperties": {},
      "properties": {
        "service": {
          "additionalProperties": {},
          "properties": {
            "port": {}
          },
          "type": "object"
        }
      }
    },
    "global": {}
  },
  "type": "object",
  "x-helm-schema-generated": true,
  "x-helm-schema-policy": {
    "annotation-format-version": 1,
    "authoring": {
      "declared-types": "annotate",
      "root": "closed"
    },
    "modifiers": {
      "overrides": {
        "count": 0,
        "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
      },
      "reference-mode": "bundled"
    },
    "narrowing": [],
    "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
    "policy-vocabulary-version": 2,
    "requested-profile": "full",
    "resolved": {
      "kind-partitions": true,
      "local-conditionals": true,
      "root-anchored-conditionals": true,
      "terminal-clauses": true
    }
  }
}
"#};

fn schema_for(values: &str, program: &str) -> eyre::Result<Value> {
    let chart_dir = VfsPath::new(vfs::MemoryFS::new());
    // Discard every evaluated result so only evaluation can constrain the input.
    let template = formatdoc! {"
        {program}
        apiVersion: v1
        kind: ConfigMap
        metadata:
          name: member-hosts
    "};
    for (path, source) in [
        (
            "Chart.yaml",
            indoc! {"
            apiVersion: v2
            name: member-hosts
            version: 0.1.0
        "},
        ),
        ("values.yaml", values),
        ("templates/probe.yaml", template.as_str()),
    ] {
        test_util::write(&chart_dir.join(path)?, source)?;
    }
    let session = AnalysisSession::new(GenerateOptions {
        chart_dir,
        include_tests: false,
        include_subchart_values: true,
        values_files: Vec::new(),
        infer_required: false,
        emission: SchemaProfile::Full.into(),
        authoring: AuthoringPolicy {
            declared_types: DeclaredTypes::Annotate,
            ..Default::default()
        },
        provider: ProviderOptions {
            disable_k8s_schemas: true,
            allow_net: false,
            crd_catalog_cache_dir: Some(test_util::cold_provider_cache_root("crd")?),
            ..Default::default()
        },
    });
    let schema = session.emit(EmitRequest {
        reference_policy: ReferencePolicy::SelfContained,
        output: OutputPipelineOptions {
            strip_descriptions: false,
            minimize: true,
            definition_names: helm_schema::output::DefinitionNames::Source,
        },
    })?;
    Ok(schema)
}

fn member_documents(expected: &Value) -> Vec<Value> {
    let candidates = vec![
        json!(null),
        json!("x"),
        json!(true),
        json!(false),
        json!(0),
        json!(""),
        json!([]),
        json!({}),
        json!(1),
        json!(-1),
        json!([1]),
        json!({"b": 1}),
    ];
    let mut documents = vec![json!({})];
    if let Some(properties) = expected.get("properties").and_then(Value::as_object) {
        for (host, shape) in properties {
            if host == "global" || host == "enabled" {
                continue;
            }
            for member in &candidates {
                documents.push(json!({host: {"unknown": member}}));
                documents.push(json!({host: [member]}));
            }
            for candidate in &candidates {
                documents.push(json!({host: candidate}));
            }
            if let Some(children) = shape.get("properties").and_then(Value::as_object) {
                for child in children.keys() {
                    for candidate in &candidates {
                        documents.push(json!({host: {child: candidate}}));
                    }
                }
            }
        }
        if properties.contains_key("enabled") {
            let hosts = documents.clone();
            for host in hosts {
                for candidate in &candidates {
                    let mut document = host.clone();
                    if let Some(fields) = document.as_object_mut() {
                        fields.insert("enabled".to_owned(), candidate.clone());
                    }
                    documents.push(document);
                }
            }
        }
    }
    documents
}

#[derive(Clone, Copy)]
struct RangeMemberExpectation<'a> {
    host: &'a str,
    allows_nil: bool,
    guarded: bool,
}

fn expected_range_members(
    mut schema: Value,
    expectation: RangeMemberExpectation<'_>,
) -> eyre::Result<Value> {
    let RangeMemberExpectation {
        host,
        allows_nil,
        guarded,
    } = expectation;
    let member = if allows_nil {
        json!({"type": ["null", "object"]})
    } else {
        json!({"type": "object"})
    };
    let collection = json!({"anyOf": [
        {"additionalProperties": member.clone(), "type": "object"},
        {"items": member, "type": "array"},
        {"type": "null"}
    ]});
    let body = json!({"additionalProperties": {}, "properties": {host: collection}});
    let object = schema
        .as_object_mut()
        .ok_or_eyre("expected object schema")?;
    let all_of = if guarded {
        object.insert(
            "$defs".to_owned(),
            json!({"t": {"anyOf": [
                {"minProperties": 1, "type": "object"},
                {"not": {"const": 0}, "type": "number"},
                {"minLength": 1, "type": "string"},
                {"minItems": 1, "type": "array"},
                {"const": true}
            ]}}),
        );
        json!([{"if": {
            "properties": {host: {"$ref": "#/$defs/t"}},
            "required": [host],
            "type": "object"
        }, "then": body}])
    } else {
        json!([body])
    };
    object.insert("allOf".to_owned(), all_of);
    Ok(schema)
}

/// Check the semantic domain before comparing the complete emitted representation.
fn assert_member_verdicts(expected: &Value, actual: &[Value]) -> eyre::Result<()> {
    let validator = jsonschema::validator_for(expected)?;
    for (variant, schema) in actual.iter().enumerate() {
        let observed = jsonschema::validator_for(schema)?;
        for document in member_documents(expected) {
            sim_assert_eq!(
                have: observed.is_valid(&document),
                want: validator.is_valid(&document),
                "variant {variant}, coalesced document {document}"
            );
        }
    }
    Ok(())
}

#[test]
fn direct_access_and_post_access_default() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "a": {
                "type": "object"
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        },
        "required": [
            "a"
        ]
    });
    let programs = [
        r"{{ $_ := .Values.a.b }}",
        r#"{{ $_ := .Values.a.b | default "fallback" }}"#,
    ];
    let schemas = programs
        .iter()
        .map(|program| {
            schema_for(
                indoc! {r"
        a: null
    "},
                program,
            )
        })
        .collect::<eyre::Result<Vec<_>>>()?;
    assert_member_verdicts(&expected, &schemas)?;
    let encoded: Vec<Value> = serde_json::from_str(DIRECT_ACCESS_AND_POST_ACCESS_DEFAULT_ENCODED)?;
    sim_assert_eq!(have: schemas, want: encoded, "{programs:?}");
    Ok(())
}

#[test]
fn bound_and_grouped_receivers() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "a": {
                "type": [
                    "object",
                    "null"
                ]
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        }
    });
    let programs = [
        r"{{ $a := .Values.a }}{{ $_ := $a.b }}",
        r"{{ $_ := (.Values.a).b }}",
    ];
    let schemas = programs
        .iter()
        .map(|program| {
            schema_for(
                indoc! {r"
        a: null
    "},
                program,
            )
        })
        .collect::<eyre::Result<Vec<_>>>()?;
    assert_member_verdicts(&expected, &schemas)?;
    let encoded: Vec<Value> = serde_json::from_str(BOUND_AND_GROUPED_RECEIVERS_ENCODED)?;
    sim_assert_eq!(have: schemas, want: encoded, "{programs:?}");
    Ok(())
}

#[test]
fn dict_context_helper() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "a": {
                "type": "object"
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        },
        "required": [
            "a"
        ]
    });
    let program = r#"{{ define "member" }}{{ $_ := .cfg.b }}{{ end }}{{ $_ := include "member" (dict "cfg" .Values.a) }}"#;
    let schema = schema_for(
        indoc! {r"
        a: null
    "},
        program,
    )?;
    assert_member_verdicts(&expected, std::slice::from_ref(&schema))?;
    let encoded: Value = serde_json::from_str(DICT_CONTEXT_HELPER_ENCODED)?;
    sim_assert_eq!(have: schema, want: encoded);
    Ok(())
}

#[test]
fn coalesce_and_default_dict() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "a": {
                "anyOf": [
                    {
                        "enum": [
                            null,
                            false,
                            0,
                            "",
                            [],
                            {}
                        ]
                    },
                    {
                        "type": "object"
                    }
                ]
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        }
    });
    let programs = [
        r"{{ $a := coalesce .Values.a (dict) }}{{ $_ := $a.b }}",
        r"{{ $_ := (default dict .Values.a).b }}",
    ];
    let schemas = programs
        .iter()
        .map(|program| {
            schema_for(
                indoc! {r"
        a: null
    "},
                program,
            )
        })
        .collect::<eyre::Result<Vec<_>>>()?;
    assert_member_verdicts(&expected, &schemas)?;
    let encoded: Vec<Value> = serde_json::from_str(COALESCE_AND_DEFAULT_DICT_ENCODED)?;
    sim_assert_eq!(have: schemas, want: encoded, "{programs:?}");
    Ok(())
}

#[test]
fn with_if_and_nonempty_guards() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "a": {
                "anyOf": [
                    {
                        "enum": [
                            null,
                            false,
                            0,
                            "",
                            [],
                            {}
                        ]
                    },
                    {
                        "type": "object"
                    }
                ]
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        }
    });
    let programs = [
        r"{{ with .Values.a }}{{ $_ := .b }}{{ end }}",
        r"{{ if .Values.a }}{{ $_ := .Values.a.b }}{{ end }}",
        r"{{ if not (empty .Values.a) }}{{ $_ := .Values.a.b }}{{ end }}",
        r"{{ if and .Values.a .Values.a.b }}{{ end }}",
    ];
    let schemas = programs
        .iter()
        .map(|program| {
            schema_for(
                indoc! {r"
        a: null
    "},
                program,
            )
        })
        .collect::<eyre::Result<Vec<_>>>()?;
    assert_member_verdicts(&expected, &schemas)?;
    let encoded: Vec<Value> = serde_json::from_str(WITH_IF_AND_NONEMPTY_GUARDS_ENCODED)?;
    sim_assert_eq!(have: schemas, want: encoded, "{programs:?}");
    Ok(())
}

#[test]
fn independent_gate() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "a": {},
            "enabled": {},
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        },
        "if": {
            "required": [
                "enabled"
            ],
            "properties": {
                "enabled": {
                    "not": {
                        "enum": [
                            null,
                            false,
                            0,
                            "",
                            [],
                            {}
                        ]
                    }
                }
            }
        },
        "then": {
            "required": [
                "a"
            ],
            "properties": {
                "a": {
                    "type": "object"
                }
            }
        }
    });
    let programs = [
        r"{{ if .Values.enabled }}{{ $_ := .Values.a.b }}{{ end }}",
        r"{{ if and .Values.enabled .Values.a.b }}{{ end }}",
    ];
    let schemas = programs
        .iter()
        .map(|program| {
            schema_for(
                indoc! {r"
        a: null
        enabled: false
    "},
                program,
            )
        })
        .collect::<eyre::Result<Vec<_>>>()?;
    assert_member_verdicts(&expected, &schemas)?;
    let encoded: Vec<Value> = serde_json::from_str(INDEPENDENT_GATE_ENCODED)?;
    sim_assert_eq!(have: schemas, want: encoded, "{programs:?}");
    Ok(())
}

#[test]
fn has_key_protects_absence_only() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "a": {
                "type": "object"
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        }
    });
    let program = r#"{{ if hasKey .Values "a" }}{{ $_ := .Values.a.b }}{{ end }}"#;
    let schema = schema_for(
        indoc! {r"
        a: null
    "},
        program,
    )?;
    assert_member_verdicts(&expected, std::slice::from_ref(&schema))?;
    let encoded: Value = serde_json::from_str(HAS_KEY_PROTECTS_ABSENCE_ONLY_ENCODED)?;
    sim_assert_eq!(have: schema, want: encoded);
    Ok(())
}

#[test]
fn index_requires_host_but_not_final_member() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "a": {
                "type": "object"
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        },
        "required": [
            "a"
        ]
    });
    let program = r#"{{ $_ := index .Values.a "b" }}"#;
    let schema = schema_for(
        indoc! {r"
        a: null
    "},
        program,
    )?;
    assert_member_verdicts(&expected, std::slice::from_ref(&schema))?;
    let encoded: Vec<Value> = serde_json::from_str(DIRECT_ACCESS_AND_POST_ACCESS_DEFAULT_ENCODED)?;
    sim_assert_eq!(have: schema, want: encoded[0]);
    Ok(())
}

#[test]
fn map_range_member_hosts() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "rows": {},
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        }
    });
    // Helm 4.2.3 renders empty arrays.
    // A direct range binding still aborts on nil members.
    for (program, allows_nil) in [
        (
            r"{{ range $key, $member := .Values.rows }}{{ $_ := $member.b }}{{ end }}",
            false,
        ),
        (
            r"{{ range $key, $member := .Values.rows }}{{ $_ := ($member).b }}{{ end }}",
            true,
        ),
        (
            r"{{ range $key, $member := .Values.rows }}{{ $copy := $member }}{{ $_ := $copy.b }}{{ end }}",
            true,
        ),
    ] {
        let expected = expected_range_members(
            expected.clone(),
            RangeMemberExpectation {
                host: "rows",
                allows_nil,
                guarded: false,
            },
        )?;
        let schema = schema_for("rows: {}\n", program)?;
        assert_member_verdicts(&expected, std::slice::from_ref(&schema))?;
        sim_assert_eq!(have: schema, want: expected, "{program}");
    }
    Ok(())
}

#[test]
fn merge_preserves_unrelated_bound_member() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "a": {
                "type": "object"
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        },
        "required": [
            "a"
        ]
    });
    let program =
        r#"{{ $ctx := merge (dict "cfg" .Values.a) (dict "z" 1) }}{{ $_ := $ctx.cfg.b }}"#;
    let schema = schema_for(
        indoc! {r"
        a: null
    "},
        program,
    )?;
    assert_member_verdicts(&expected, std::slice::from_ref(&schema))?;
    let encoded: Vec<Value> = serde_json::from_str(DIRECT_ACCESS_AND_POST_ACCESS_DEFAULT_ENCODED)?;
    sim_assert_eq!(have: schema, want: encoded[0]);
    Ok(())
}

#[test]
fn set_replacement_does_not_constrain_original() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "a": {},
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        }
    });
    let program = r#"{{ $_ := set .Values "a" (dict "b" 1) }}{{ $_ := .Values.a.b }}"#;
    let schema = schema_for(
        indoc! {r"
        a: null
    "},
        program,
    )?;
    assert_member_verdicts(&expected, std::slice::from_ref(&schema))?;
    sim_assert_eq!(have: schema, want: expected);
    Ok(())
}

#[test]
fn literal_tpl_and_helper_equivalence() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "a": {
                "type": "object"
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        },
        "required": [
            "a"
        ]
    });
    let programs = [
        r#"{{ $_ := tpl "{{ $_ := .Values.a.b }}" . }}"#,
        r#"{{ define "member" }}{{ $_ := .Values.a.b }}{{ end }}{{ $_ := include "member" . }}"#,
    ];
    let schemas = programs
        .iter()
        .map(|program| {
            schema_for(
                indoc! {r"
        a: null
    "},
                program,
            )
        })
        .collect::<eyre::Result<Vec<_>>>()?;
    sim_assert_eq!(
            have: schema_for("program: null", r"{{ $_ := tpl (toString .Values.program) . }}")?,
            want: json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "global": {},
            "program": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        }
    })
        );
    assert_member_verdicts(&expected, &schemas)?;
    let encoded: Vec<Value> = serde_json::from_str(LITERAL_TPL_AND_HELPER_EQUIVALENCE_ENCODED)?;
    sim_assert_eq!(have: schemas, want: encoded, "{programs:?}");
    Ok(())
}

#[test]
fn redis_auth_acl_enabled() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "auth": {
                "type": "object",
                "required": [
                    "acl"
                ],
                "properties": {
                    "acl": {
                        "type": "object"
                    }
                }
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        },
        "required": [
            "auth"
        ]
    });
    let program = r"{{ if .Values.auth.acl.enabled }}{{ end }}";
    let schema = schema_for(
        indoc! {r"
        auth: null
    "},
        program,
    )?;
    assert_member_verdicts(&expected, std::slice::from_ref(&schema))?;
    let encoded: Value = serde_json::from_str(REDIS_AUTH_ACL_ENABLED_ENCODED)?;
    sim_assert_eq!(have: schema, want: encoded);
    Ok(())
}

#[test]
fn argo_cd_api_version_overrides_monitoring() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "apiVersionOverrides": {
                "type": "object"
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        },
        "required": [
            "apiVersionOverrides"
        ]
    });
    let program = r#"{{ define "monitoring" }}{{ if .Values.apiVersionOverrides.monitoring }}{{ else }}fixed{{ end }}{{ end }}{{ $_ := include "monitoring" . }}"#;
    let schema = schema_for(
        indoc! {r"
        apiVersionOverrides: null
    "},
        program,
    )?;
    assert_member_verdicts(&expected, std::slice::from_ref(&schema))?;
    let encoded: Value = serde_json::from_str(ARGO_CD_API_VERSION_OVERRIDES_MONITORING_ENCODED)?;
    sim_assert_eq!(have: schema, want: encoded);
    Ok(())
}

#[test]
fn cilium_index_in_and() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "extraConfig": {
                "type": "object"
            },
            "enabled": {},
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        },
        "required": [
            "extraConfig"
        ]
    });
    let program =
        r#"{{ if and (index .Values.extraConfig "bpf-lb-mode") .Values.enabled }}{{ end }}"#;
    let schema = schema_for(
        indoc! {r"
        extraConfig: null
        enabled: false
    "},
        program,
    )?;
    assert_member_verdicts(&expected, std::slice::from_ref(&schema))?;
    let encoded: Value = serde_json::from_str(CILIUM_INDEX_IN_AND_ENCODED)?;
    sim_assert_eq!(have: schema, want: encoded);
    Ok(())
}

#[test]
fn kps_range_member() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "additionalPrometheusRulesMap": {},
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        }
    });
    // Helm 4.2.3 skips false, zero, empty strings and empty arrays at the outer if.
    let expected = expected_range_members(
        expected,
        RangeMemberExpectation {
            host: "additionalPrometheusRulesMap",
            allows_nil: false,
            guarded: true,
        },
    )?;
    let program = r"{{ if .Values.additionalPrometheusRulesMap }}{{ range $name, $rule := .Values.additionalPrometheusRulesMap }}{{ $_ := $rule.additionalLabels }}{{ end }}{{ end }}";
    let schema = schema_for(
        indoc! {r"
        additionalPrometheusRulesMap: {}
    "},
        program,
    )?;
    assert_member_verdicts(&expected, std::slice::from_ref(&schema))?;
    sim_assert_eq!(have: schema, want: expected);
    Ok(())
}

#[test]
fn milvus_nested_include() -> eyre::Result<()> {
    let expected = json!({
        "$schema": "http://json-schema.org/draft-07/schema#",
        "additionalProperties": false,
        "properties": {
            "dataCoordinator": {
                "type": "object",
                "required": [
                    "service"
                ],
                "properties": {
                    "service": {
                        "type": "object"
                    }
                }
            },
            "global": {}
        },
        "type": "object",
        "x-helm-schema-generated": true,
        "x-helm-schema-policy": {
            "annotation-format-version": 1,
            "authoring": {
                "declared-types": "annotate",
                "root": "closed"
            },
            "modifiers": {
                "overrides": {
                    "count": 0,
                    "digest": "4f53cda18c2baa0c0354bb5f9a3ecbe5ed12ab4d8e11ba873c2f11161202b945"
                },
                "reference-mode": "bundled"
            },
            "narrowing": [],
            "policy-fingerprint": "b28fd01d710d87673d204ba4cdf8333388b4a8086cba088db9d9751d42e7c9cf",
            "policy-vocabulary-version": 2,
            "requested-profile": "full",
            "resolved": {
                "kind-partitions": true,
                "local-conditionals": true,
                "root-anchored-conditionals": true,
                "terminal-clauses": true
            }
        },
        "required": [
            "dataCoordinator"
        ]
    });
    let program = r#"{{ define "config" }}{{ $_ := .Values.dataCoordinator.service.port }}{{ end }}{{ define "outer" }}{{ include "config" . }}{{ end }}{{ $_ := include "outer" . }}"#;
    let schema = schema_for(
        indoc! {r"
        dataCoordinator: null
    "},
        program,
    )?;
    assert_member_verdicts(&expected, std::slice::from_ref(&schema))?;
    let encoded: Value = serde_json::from_str(MILVUS_NESTED_INCLUDE_ENCODED)?;
    sim_assert_eq!(have: schema, want: encoded);
    Ok(())
}
