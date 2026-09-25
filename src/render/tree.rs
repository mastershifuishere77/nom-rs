pub struct TreeNode<T> {
    pub label: String,
    pub extra: T,
    pub children: Vec<TreeNode<T>>,
}

const BRANCH_LAST: &str = "\x1b[34m┌─ \x1b[0m";
const BRANCH_MID: &str = "\x1b[34m├─ \x1b[0m";
const CONT_LAST: &str = "   ";
const CONT_MID: &str = "\x1b[34m│  \x1b[0m";

pub fn show_forest<T: Clone>(forest: &[TreeNode<T>]) -> Vec<(String, T)> {
    let mut rows = Vec::new();
    let mut prefix = String::new();
    for node in forest {
        rows.push((node.label.clone(), node.extra.clone()));
        let num_children = node.children.len();
        for (i, child) in node.children.iter().enumerate() {
            let is_last = i + 1 == num_children;
            render_subtree(child, &mut prefix, is_last, &mut rows);
        }
    }
    rows.reverse();
    rows
}

fn render_subtree<T: Clone>(
    node: &TreeNode<T>,
    prefix: &mut String,
    is_last: bool,
    out: &mut Vec<(String, T)>,
) {
    let prev_len = prefix.len();
    prefix.push_str(if is_last { BRANCH_LAST } else { BRANCH_MID });

    let mut row_str = String::with_capacity(prefix.len() + node.label.len());
    row_str.push_str(prefix);
    row_str.push_str(&node.label);
    out.push((row_str, node.extra.clone()));

    prefix.truncate(prev_len);
    prefix.push_str(if is_last { CONT_LAST } else { CONT_MID });

    let num_children = node.children.len();
    for (i, child) in node.children.iter().enumerate() {
        let child_is_last = i + 1 == num_children;
        render_subtree(child, prefix, child_is_last, out);
    }

    prefix.truncate(prev_len);
}
