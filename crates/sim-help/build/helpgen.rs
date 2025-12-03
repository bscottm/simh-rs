// SPDX-License-Identifier: MIT

use std::env;
use std::fs;
use std::path::Path;

mod help_generator {
    use serde::Deserialize;
    use std::collections::HashMap;

    #[derive(Deserialize, Debug)]
    pub struct HelpData {
        pub root: RootNode,
        pub nodes: Vec<HelpNode>,
        pub command_map: HashMap<String, String>,
        pub command_subsections: Option<HashMap<String, HashMap<String, String>>>,
        pub topics: Option<Vec<Topic>>,
        pub see_also: Option<HashMap<String, Vec<String>>>,
    }

    #[derive(Deserialize, Debug)]
    pub struct RootNode {
        pub id: String,
        pub title: String,
        pub content: String,
    }

    #[derive(Deserialize, Debug)]
    pub struct HelpNode {
        pub id: String,
        pub parent: String,
        pub title: String,
        pub content: String,
        #[serde(default)]
        pub children: Vec<String>,
    }

    #[derive(Deserialize, Debug)]
    pub struct Topic {
        pub name: String,
        pub title: String,
        pub description: String,
        pub nodes: Vec<String>,
    }

    pub fn generate_help_code(data: &HelpData) -> String {
        let mut code = String::from(
            r#"// Auto-generated help data - do not edit manually

// See data declarations and functions in sim-help/src/helpfuncs.rs

pub static HELP_SYSTEM: HelpSystem = HelpSystem {
"#,
        );

        code.push_str(&format!("    root_id: \"{}\",\n", data.root.id));
        code.push_str(&format!(
            "    root_title: \"{}\",\n",
            escape_string(&data.root.title)
        ));
        code.push_str(&format!("    root_content: r#\"{}\"#,\n", data.root.content));
        code.push_str("};\n\n");

        code.push_str("pub static HELP_NODES: &[HelpNode] = &[\n");
        for node in &data.nodes {
            code.push_str("    HelpNode {\n");
            code.push_str(&format!("        id: \"{}\",\n", node.id));
            code.push_str(&format!("        parent: Some(\"{}\"),\n", node.parent));
            code.push_str(&format!("        title: \"{}\",\n", escape_string(&node.title)));
            code.push_str(&format!("        content: r#\"{}\"#,\n", node.content));
            code.push_str("        children: &[\n");
            for child_id in &node.children {
                code.push_str(&format!("            \"{}\",\n", child_id));
            }
            code.push_str("        ],\n");
            code.push_str("    },\n");
        }
        code.push_str("];\n\n");

        code.push_str("pub static COMMAND_MAP: &[CommandMapping] = &[\n");
        for (command, path) in &data.command_map {
            code.push_str("    CommandMapping {\n");
            code.push_str(&format!("        command: \"{}\",\n", command.to_uppercase()));
            code.push_str(&format!("        help_path: \"{}\",\n", path));
            code.push_str("    },\n");
        }
        code.push_str("];\n\n");

        if let Some(subsections) = &data.command_subsections {
            code.push_str("pub static COMMAND_SUBSECTIONS: &[CommandSubsection] = &[\n");
            for (command, subs) in subsections {
                for (subsection, path) in subs {
                    code.push_str("    CommandSubsection {\n");
                    code.push_str(&format!("        command: \"{}\",\n", command.to_uppercase()));
                    code.push_str(&format!("        subsection: \"{}\",\n", subsection));
                    code.push_str(&format!("        help_path: \"{}\",\n", path));
                    code.push_str("    },\n");
                }
            }
            code.push_str("];\n\n");
        } else {
            code.push_str("pub static COMMAND_SUBSECTIONS: &[CommandSubsection] = &[];\n\n");
        }

        if let Some(topics) = &data.topics {
            code.push_str("pub static TOPICS: &[Topic] = &[\n");
            for topic in topics {
                code.push_str("    Topic {\n");
                code.push_str(&format!("        name: \"{}\",\n", topic.name));
                code.push_str(&format!("        title: \"{}\",\n", escape_string(&topic.title)));
                code.push_str(&format!(
                    "        description: \"{}\",\n",
                    escape_string(&topic.description)
                ));
                code.push_str("        nodes: &[\n");
                for node_path in &topic.nodes {
                    code.push_str(&format!("            \"{}\",\n", node_path));
                }
                code.push_str("        ],\n");
                code.push_str("    },\n");
            }
            code.push_str("];\n\n");
        } else {
            code.push_str("pub static TOPICS: &[Topic] = &[];\n\n");
        }

        if let Some(see_also) = &data.see_also {
            code.push_str("pub static SEE_ALSO: &[(&str, &[&str])] = &[\n");
            for (node_path, related) in see_also {
                code.push_str(&format!("    (\"{}\", &[\n", node_path));
                for related_path in related {
                    code.push_str(&format!("        \"{}\",\n", related_path));
                }
                code.push_str("    ]),\n");
            }
            code.push_str("];\n\n");
        } else {
            code.push_str("pub static SEE_ALSO: &[(&str, &[&str])] = &[];\n\n");
        }

        code
    }

    fn escape_string(s: &str) -> String {
        s.replace('\\', "\\\\").replace('"', "\\\"")
    }
}

pub fn do_generate_help() {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("help_data.rs");

    let yaml_content = fs::read_to_string("simhelp.yaml").expect("Failed to read help.yaml");

    let help_data: help_generator::HelpData =
        serde_yaml_ng::from_str(&yaml_content).expect("Failed to parse help.yaml");

    let generated_code = help_generator::generate_help_code(&help_data);

    fs::write(&dest_path, generated_code).expect("Failed to write generated help code");

    println!("cargo:rerun-if-changed=simhelp.yaml");
}
