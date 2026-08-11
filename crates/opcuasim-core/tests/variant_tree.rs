//! Integration tests for variant_to_tree – structured Variant → TreeNode conversion.
//! Tests cover: scalars, 1D/2D arrays, ExtensionObject fallback, nested combos, empty.

use opcua_types::{Array, ExtensionObject, UAString, Variant, VariantScalarTypeId};
use opcuasim_core::values::{variant_to_tree, TreeNode};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn leaf(name: &str, value: &str) -> TreeNode {
    TreeNode {
        name: name.to_string(),
        value: value.to_string(),
        children: vec![],
    }
}

// ---------------------------------------------------------------------------
// Scalars
// ---------------------------------------------------------------------------

#[test]
fn scalar_int32() {
    let result = variant_to_tree("MyInt", &Variant::Int32(42));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], leaf("MyInt", "42"));
}

#[test]
fn scalar_double() {
    let result = variant_to_tree("MyDouble", &Variant::Double(3.5));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], leaf("MyDouble", "3.5"));
}

#[test]
fn scalar_string() {
    let result = variant_to_tree("MyStr", &Variant::String(UAString::from("hello")));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], leaf("MyStr", "hello"));
}

#[test]
fn scalar_boolean() {
    let result = variant_to_tree("MyBool", &Variant::Boolean(true));
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], leaf("MyBool", "true"));
}

#[test]
fn empty_variant() {
    let result = variant_to_tree("EmptyField", &Variant::Empty);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], leaf("EmptyField", ""));
}

// ---------------------------------------------------------------------------
// 1D arrays
// ---------------------------------------------------------------------------

#[test]
fn array_1d_int32() {
    let values = vec![Variant::Int32(1), Variant::Int32(2), Variant::Int32(3)];
    let arr = Array::new(VariantScalarTypeId::Int32, values).expect("valid 1D array");
    let result = variant_to_tree("Arr", &Variant::Array(Box::new(arr)));

    assert_eq!(result.len(), 1);
    let root = &result[0];
    assert_eq!(root.name, "Arr");
    assert_eq!(root.children.len(), 3);
    assert_eq!(root.children[0], leaf("[0]", "1"));
    assert_eq!(root.children[1], leaf("[1]", "2"));
    assert_eq!(root.children[2], leaf("[2]", "3"));
}

#[test]
fn array_1d_empty() {
    let arr = Array::new(VariantScalarTypeId::Int32, vec![]).expect("valid empty array");
    let result = variant_to_tree("EmptyArr", &Variant::Array(Box::new(arr)));

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].name, "EmptyArr");
    assert_eq!(result[0].children.len(), 0);
}

// ---------------------------------------------------------------------------
// 2D arrays
// ---------------------------------------------------------------------------

#[test]
fn array_2d_int32() {
    // 2 rows × 3 cols = [1, 2, 3, 4, 5, 6]
    let values: Vec<Variant> = (1..=6).map(Variant::Int32).collect();
    let arr =
        Array::new_multi(VariantScalarTypeId::Int32, values, vec![2, 3]).expect("valid 2D array");
    let result = variant_to_tree("Grid", &Variant::Array(Box::new(arr)));

    assert_eq!(result.len(), 1);
    let root = &result[0];
    assert_eq!(root.name, "Grid");
    assert_eq!(root.children.len(), 2); // 2 rows

    // Row 0
    let row0 = &root.children[0];
    assert_eq!(row0.name, "[0]");
    assert_eq!(row0.children.len(), 3);
    assert_eq!(row0.children[0], leaf("[0]", "1"));
    assert_eq!(row0.children[1], leaf("[1]", "2"));
    assert_eq!(row0.children[2], leaf("[2]", "3"));

    // Row 1
    let row1 = &root.children[1];
    assert_eq!(row1.name, "[1]");
    assert_eq!(row1.children.len(), 3);
    assert_eq!(row1.children[0], leaf("[0]", "4"));
    assert_eq!(row1.children[1], leaf("[1]", "5"));
    assert_eq!(row1.children[2], leaf("[2]", "6"));
}

// ---------------------------------------------------------------------------
// ExtensionObject fallback (not decoded as DynamicStructure)
// ---------------------------------------------------------------------------

#[test]
fn extension_object_null_fallback() {
    // ExtensionObject::null() – body is None, inner_as::<DynamicStructure>() returns None
    let eo = ExtensionObject::null();
    let result = variant_to_tree("StructField", &Variant::ExtensionObject(eo));

    assert_eq!(result.len(), 1);
    let root = &result[0];
    assert_eq!(root.name, "StructField");
    // Fallback: leaf node, value is Display output (contains type info)
    assert!(!root.value.is_empty(), "fallback should show type info");
    assert_eq!(root.children.len(), 0);
}

// ---------------------------------------------------------------------------
// Nested: 1D array of ExtensionObjects (each fallback)
// ---------------------------------------------------------------------------

#[test]
fn array_1d_of_extension_objects() {
    let eo1 = ExtensionObject::null();
    let eo2 = ExtensionObject::null();
    let values = vec![Variant::ExtensionObject(eo1), Variant::ExtensionObject(eo2)];
    let arr = Array::new(VariantScalarTypeId::ExtensionObject, values).expect("valid array of EO");
    let result = variant_to_tree("EOArr", &Variant::Array(Box::new(arr)));

    assert_eq!(result.len(), 1);
    let root = &result[0];
    assert_eq!(root.name, "EOArr");
    assert_eq!(root.children.len(), 2);

    let child0 = &root.children[0];
    assert_eq!(child0.name, "[0]");
    assert!(!child0.value.is_empty());
    assert_eq!(child0.children.len(), 0);

    let child1 = &root.children[1];
    assert_eq!(child1.name, "[1]");
    assert!(!child1.value.is_empty());
    assert_eq!(child1.children.len(), 0);
}
