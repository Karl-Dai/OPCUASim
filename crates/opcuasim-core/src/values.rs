//! 结构化 Variant 值 → 树节点(主站字段树展示的纯函数基础)

use opcua_types::custom::DynamicStructure;
use opcua_types::{Array, Variant};

/// 单棵树节点:字段名/元素索引 + 显示文本 + 子节点。
#[derive(Debug, Clone, PartialEq)]
pub struct TreeNode {
    pub name: String,
    pub value: String,
    pub children: Vec<TreeNode>,
}

/// 递归把 Variant 转成树,DEFAULT 根名由其调用方传递(name 参数)。
/// 顶层返回单节点列表(通常 1 个:name = 传入名,value = 标量显示 / 摘要,children = 复杂展开)。
pub fn variant_to_tree(name: &str, v: &Variant) -> Vec<TreeNode> {
    let (value, children) = match v {
        Variant::Empty => (String::new(), vec![]),
        Variant::Array(arr) => (String::new(), build_array_children(arr)),
        Variant::ExtensionObject(eo) => {
            if let Some(ds) = eo.inner_as::<DynamicStructure>() {
                // 解码成功:按字段展开
                (String::new(), build_structure_children(ds))
            } else {
                // Fallback:未解码或非结构体 ExtensionObject → 叶子节点
                // 值显示库的 Display 输出(含类型标注),客户端未解码时显示原始编码
                (format!("{v}"), vec![])
            }
        }
        _ => (format!("{v}"), vec![]),
    };
    vec![TreeNode {
        name: name.to_string(),
        value,
        children,
    }]
}

// ---------------------------------------------------------------------------
// 内部 helpers
// ---------------------------------------------------------------------------

/// 递归展开 Variant 的子节点(用于数组元素 / 结构体字段)。
fn variant_children(v: &Variant) -> Vec<TreeNode> {
    match v {
        Variant::Array(arr) => build_array_children(arr),
        Variant::ExtensionObject(eo) => {
            if let Some(ds) = eo.inner_as::<DynamicStructure>() {
                build_structure_children(ds)
            } else {
                vec![]
            }
        }
        _ => vec![],
    }
}

/// 构建数组子节点:
/// - 无 dimensions 或一维:每元素一个子节点
/// - dimensions 二维 [rows, cols]:按行分组
/// - 维度 ≥3:按一维展开(在摘要注明)
fn build_array_children(arr: &Array) -> Vec<TreeNode> {
    if let Some(dims) = &arr.dimensions {
        if dims.len() == 2 {
            return build_2d_array_children(arr, dims[0] as usize, dims[1] as usize);
        }
        // dims.len() >= 3:按一维展开
    }
    // 一维(或无 dimensions):逐元素展开
    arr.values
        .iter()
        .enumerate()
        .map(|(i, val)| TreeNode {
            name: format!("[{i}]"),
            value: format!("{val}"),
            children: variant_children(val),
        })
        .collect()
}

/// 二维数组:按行分组,每行包含该行的列元素。
fn build_2d_array_children(arr: &Array, rows: usize, cols: usize) -> Vec<TreeNode> {
    (0..rows)
        .map(|r| {
            let start = r * cols;
            let row_children: Vec<TreeNode> = (0..cols)
                .map(|c| {
                    let idx = start + c;
                    let val = arr.values.get(idx).expect("2D array index out of bounds");
                    TreeNode {
                        name: format!("[{c}]"),
                        value: format!("{val}"),
                        children: variant_children(val),
                    }
                })
                .collect();
            TreeNode {
                name: format!("[{r}]"),
                value: String::new(),
                children: row_children,
            }
        })
        .collect()
}

/// 构建 DynamicStructure 的子节点:每字段一个 TreeNode。
/// 注:type_def 字段为 pub(super),无法从外部 crate 访问字段名,
/// 因此使用索引命名(如 "[0]")。下游 UI 层若需字段名可从 type tree 补充。
fn build_structure_children(ds: &DynamicStructure) -> Vec<TreeNode> {
    ds.values()
        .iter()
        .enumerate()
        .map(|(i, val)| TreeNode {
            name: format!("[{i}]"),
            value: format!("{val}"),
            children: variant_children(val),
        })
        .collect()
}
