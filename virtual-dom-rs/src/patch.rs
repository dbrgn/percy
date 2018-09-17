use std::collections::HashMap;
use std::collections::HashSet;
use virtual_node::VirtualNode;

use wasm_bindgen::JsCast;
use web_sys;
use web_sys::{Element, Node};

/// A `Patch` encodes an operation that modifies a real DOM element.
///
/// To update the real DOM that a user sees you'll want to first diff your
/// old virtual dom and new virtual dom.
///
/// This diff operation will generate `Vec<Patch>` with zero or more patches that, when
/// applied to your real DOM, will make your real DOM look like your new virtual dom.
///
/// Each `Patch` has a u32 node index that helps us identify the real DOM node that it applies to.
///
/// Our old virtual dom's nodes are indexed depth first, as shown in this illustration
/// (0 being the root node, 1 being it's first child, 2 being it's first child's first child).
///               .─.
///              ( 0 )
///               `┬'
///           ┌────┴──────┐
///           │           │
///           ▼           ▼
///          .─.         .─.
///         ( 1 )       ( 4 )
///          `┬'         `─'
///      ┌────┴───┐       │
///      │        │       ├─────┬─────┐
///      ▼        ▼       │     │     │
///     .─.      .─.      ▼     ▼     ▼
///    ( 2 )    ( 3 )    .─.   .─.   .─.
///     `─'      `─'    ( 5 ) ( 6 ) ( 7 )
///                      `─'   `─'   `─'
///
/// We'll revisit our indexing in the future when we optimize our diff/patch process.
#[derive(Debug, PartialEq)]
pub enum Patch<'a> {
    /// Append a vector of child nodes to a parent node id.
    AppendChildren(node_idx, Vec<&'a VirtualNode>),
    /// For a `node_i32`, remove all children besides the first `len`
    TruncateChildren(node_idx, usize),
    /// Replace a node with another node. This typically happens when a node's tag changes.
    /// ex: <div> becomes <span>
    Replace(node_idx, &'a VirtualNode),
    /// Add attributes that the new node has that the old node does not
    AddAttributes(node_idx, HashMap<&'a str, &'a str>),
    /// Remove attributes that the old node had that the new node doesn't
    RemoveAttributes(node_idx, Vec<&'a str>),
    ChangeText(node_idx, &'a VirtualNode),
}

impl<'a> Patch<'a> {
    pub fn node_idx(&self) -> usize {
        match self {
            Patch::AppendChildren(node_idx, _) => *node_idx,
            Patch::TruncateChildren(node_idx, _) => *node_idx,
            Patch::Replace(node_idx, _) => *node_idx,
            Patch::AddAttributes(node_idx, _) => *node_idx,
            Patch::RemoveAttributes(node_idx, _) => *node_idx,
            Patch::ChangeText(node_idx, _) => *node_idx,
        }
    }
}

type node_idx = usize;

pub fn patch(root_node: Element, patches: &Vec<Patch>) {
    let mut cur_node_idx = 0;

    let mut nodes_to_find = HashSet::new();

    for patch in patches {
        nodes_to_find.insert(patch.node_idx());
    }

    log(&format!("NODEs TO FIND {:#?}", nodes_to_find));

    let mut elements_to_patch = HashMap::new();
    let mut text_nodes_to_patch = HashMap::new();

    find_nodes(
        root_node,
        &mut cur_node_idx,
        &mut nodes_to_find,
        &mut elements_to_patch,
        &mut text_nodes_to_patch,
    );

    for patch in patches {
        let patch_node_idx = patch.node_idx();

        log(&format!("PATCH!!! {}", patch_node_idx));
        //        log(&format!("{:#?}", patch));
        //
        //        log(&format!("{:#?}", nodes_to_patch.keys()));

        if let Some(element) = elements_to_patch.get(&patch_node_idx) {
            apply_element_patch(&element, &patch);
            continue;
        }

        if let Some(text_node) = text_nodes_to_patch.get(&patch_node_idx) {
            apply_text_patch(&text_node, &patch);
            continue;
        }

        unreachable!("Getting here means we didn't find the element or next node that we were supposed to patch.")
    }
}

use wasm_bindgen::prelude::*;
#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

// TODO: Due for a refactoring.. Should just return 2 HashMap with all of the elements and
// nodes that we find.
fn find_nodes(
    root_node: Element,
    cur_node_idx: &mut usize,
    nodes_to_find: &mut HashSet<usize>,
    nodes_to_patch: &mut HashMap<usize, Element>,
    text_nodes_to_patch: &mut HashMap<usize, web_sys::Node>,
) {
    if nodes_to_find.len() == 0 {
        return;
    }

    // We use child_nodes() instead of children() because children() ignores text nodes
    let children = (root_node.as_ref() as &web_sys::Node).child_nodes();
    let child_node_count = children.length();

    if nodes_to_find.get(&cur_node_idx).is_some() {
        nodes_to_patch.insert(*cur_node_idx, root_node);
        nodes_to_find.remove(&cur_node_idx);
    }

    *cur_node_idx += 1;

    log(&format!("CHIlD NODE COUNT {}", child_node_count));

    for i in 0..child_node_count {
        let node = children.item(i).unwrap().dyn_into::<Element>();

        if node.is_ok() {
            find_nodes(
                node.ok().unwrap(),
                cur_node_idx,
                nodes_to_find,
                nodes_to_patch,
                text_nodes_to_patch,
            );
        } else {
            text_nodes_to_patch.insert(*cur_node_idx, node.err().unwrap());
        }
    }
}

fn apply_element_patch(node: &Element, patch: &Patch) {
    match patch {
        Patch::AddAttributes(_node_idx, attributes) => {
            for (attrib_name, attrib_val) in attributes.iter() {
                node.set_attribute(attrib_name, attrib_val)
                    .expect("Set attribute on element");
            }
        }
        Patch::RemoveAttributes(_node_idx, attributes) => {
            for attrib_name in attributes.iter() {
                node.remove_attribute(attrib_name);
            }
        }
        Patch::Replace(_node_idx, new_node) => {
            node.replace_with_with_node_1(new_node.create_element().as_ref() as &web_sys::Node)
                .expect("Replaced element");
        }
        Patch::TruncateChildren(_node_idx, len) => {
            let children = node.children();
            let count = children.length();

            for index in *len as u32..count {
                let child_to_remove = children.get_with_index(index).unwrap();

                (node.as_ref() as &web_sys::Node)
                    .remove_child(child_to_remove.as_ref() as &web_sys::Node)
                    .expect("Truncated children");
            }
        }
        Patch::AppendChildren(_node_idx, new_nodes) => {
            for new_node in new_nodes {
                (node.as_ref() as &web_sys::Node)
                    .append_child(new_node.create_element().as_ref() as &web_sys::Node)
                    .expect("Appended child element");
            }
        }
        Patch::ChangeText(_node_idx, new_node) => unreachable!(
            "Elements should not receive ChangeText patches. Those should go to Node's"
        ),
    }
}

fn apply_text_patch(node: &Node, patch: &Patch) {
    match patch {
        Patch::ChangeText(_node_idx, new_node) => {
            let text = new_node.text.as_ref().expect("New text to use");

            node.set_node_value(Some(text.as_str()));
        }
        _ => unreachable!(
            "Node's should only receive change text patches. All other patches go to Element's"
        ),
    }
}
