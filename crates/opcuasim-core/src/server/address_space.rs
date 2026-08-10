use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use log::{info, warn};
use opcua_nodes::{
    DataTypeBuilder, DefaultTypeTree, ObjectBuilder, ReferenceDirection, VariableBuilder,
};
use opcua_server::address_space::AddressSpace;
use opcua_types::{
    custom::{DataTypeTree, DynamicStructure, EncodingIds, ParentIds, TypeInfo},
    Array, DataTypeDefinition, DataTypeId, EnumDefinition, EnumField, ExtensionObject, Identifier,
    LocalizedText, NodeClass, NodeId, ObjectTypeId, QualifiedName, ReferenceTypeId,
    StructureDefinition, StructureField, StructureType, UAString, Variant, VariantScalarTypeId,
};

use super::models::{DataType, ServerFolder, ServerNode, SimulationMode, StructField};
use crate::error::OpcUaSimError;

const DEFAULT_ARRAY_LEN: usize = 4;

fn data_type_to_node_id(
    dt: &DataType,
    custom: &HashMap<String, NodeId>,
) -> NodeId {
    dt.type_node_id(custom)
}

fn data_type_scalar_id(dt: &DataType) -> VariantScalarTypeId {
    match dt {
        DataType::Boolean => VariantScalarTypeId::Boolean,
        DataType::Int16 => VariantScalarTypeId::Int16,
        DataType::Int32 => VariantScalarTypeId::Int32,
        DataType::Int64 => VariantScalarTypeId::Int64,
        DataType::UInt16 => VariantScalarTypeId::UInt16,
        DataType::UInt32 => VariantScalarTypeId::UInt32,
        DataType::UInt64 => VariantScalarTypeId::UInt64,
        DataType::Float => VariantScalarTypeId::Float,
        DataType::Double => VariantScalarTypeId::Double,
        DataType::String => VariantScalarTypeId::String,
        DataType::DateTime => VariantScalarTypeId::DateTime,
        DataType::ByteString => VariantScalarTypeId::ByteString,
        DataType::Array { element_type } | DataType::Array2D { element_type, .. } => {
            data_type_scalar_id(element_type)
        }
        DataType::Enum { .. } => VariantScalarTypeId::Int32,
        DataType::Structure { .. } => VariantScalarTypeId::ExtensionObject,
    }
}

fn scalar_id_to_default_variant(scalar: VariantScalarTypeId) -> Variant {
    match scalar {
        VariantScalarTypeId::Boolean => Variant::Boolean(false),
        VariantScalarTypeId::Int16 => Variant::Int16(0),
        VariantScalarTypeId::Int32 => Variant::Int32(0),
        VariantScalarTypeId::Int64 => Variant::Int64(0),
        VariantScalarTypeId::UInt16 => Variant::UInt16(0),
        VariantScalarTypeId::UInt32 => Variant::UInt32(0),
        VariantScalarTypeId::UInt64 => Variant::UInt64(0),
        VariantScalarTypeId::Float => Variant::Float(0.0),
        VariantScalarTypeId::Double => Variant::Double(0.0),
        VariantScalarTypeId::String => Variant::String(UAString::from("")),
        VariantScalarTypeId::DateTime => Variant::Double(0.0),
        VariantScalarTypeId::ByteString => Variant::String(UAString::from("")),
        _ => Variant::Empty,
    }
}

fn initial_value_for_data_type(dt: &DataType) -> Variant {
    match dt {
        DataType::Array { element_type } => {
            let scalar = data_type_scalar_id(element_type);
            let element = scalar_id_to_default_variant(scalar);
            let values: Vec<Variant> = (0..4).map(|_| element.clone()).collect();
            if let Ok(arr) = Array::new(scalar, values) {
                Variant::Array(Box::new(arr))
            } else {
                Variant::Empty
            }
        }
        DataType::Array2D {
            element_type,
            dims,
        } => {
            let scalar = data_type_scalar_id(element_type);
            let element = scalar_id_to_default_variant(scalar);
            let count = (dims[0] as usize) * (dims[1] as usize);
            let values: Vec<Variant> = (0..count).map(|_| element.clone()).collect();
            if let Ok(arr) = Array::new_multi(scalar, values, vec![dims[0], dims[1]]) {
                Variant::Array(Box::new(arr))
            } else {
                Variant::Empty
            }
        }
        DataType::Enum { fields, .. } => {
            let first_value = fields.first().map(|(v, _)| *v).unwrap_or(0);
            Variant::Int32(first_value as i32)
        }
        DataType::Structure { .. } => Variant::ExtensionObject(ExtensionObject::null()),
        _ => scalar_id_to_default_variant(data_type_scalar_id(dt)),
    }
}

/// Convert a string value to a Variant for the given data type.
pub fn string_to_variant(value: &str, data_type: &DataType, custom: &HashMap<String, NodeId>) -> Variant {
    match data_type {
        DataType::Boolean => Variant::Boolean(value.eq_ignore_ascii_case("true") || value == "1"),
        DataType::Int16 => value
            .parse::<i16>()
            .map(Variant::Int16)
            .unwrap_or(Variant::Int16(0)),
        DataType::Int32 => value
            .parse::<i32>()
            .map(Variant::Int32)
            .unwrap_or(Variant::Int32(0)),
        DataType::Int64 => value
            .parse::<i64>()
            .map(Variant::Int64)
            .unwrap_or(Variant::Int64(0)),
        DataType::UInt16 => value
            .parse::<u16>()
            .map(Variant::UInt16)
            .unwrap_or(Variant::UInt16(0)),
        DataType::UInt32 => value
            .parse::<u32>()
            .map(Variant::UInt32)
            .unwrap_or(Variant::UInt32(0)),
        DataType::UInt64 => value
            .parse::<u64>()
            .map(Variant::UInt64)
            .unwrap_or(Variant::UInt64(0)),
        DataType::Float => value
            .parse::<f32>()
            .map(Variant::Float)
            .unwrap_or(Variant::Float(0.0)),
        DataType::Double => value
            .parse::<f64>()
            .map(Variant::Double)
            .unwrap_or(Variant::Double(0.0)),
        DataType::String => Variant::String(UAString::from(value)),
        DataType::DateTime => Variant::String(UAString::from(value)),
        DataType::ByteString => Variant::String(UAString::from(value)),
        DataType::Enum { fields, .. } => {
            let lower = value.trim().to_ascii_lowercase();
            if let Some((v, _)) = fields.iter().find(|(_, name)| name.to_ascii_lowercase() == lower) {
                Variant::Int32(*v as i32)
            } else {
                value
                    .parse::<i32>()
                    .map(Variant::Int32)
                    .unwrap_or_else(|_| initial_value_for_data_type(data_type))
            }
        }
        DataType::Array { element_type } => {
            let scalar = data_type_scalar_id(element_type);
            let values: Vec<Variant> = value
                .split(',')
                .map(|s| string_to_variant(s.trim(), element_type, custom))
                .collect();
            if values.is_empty() {
                initial_value_for_data_type(data_type)
            } else {
                Array::new(scalar, values)
                    .map(|arr| Variant::Array(Box::new(arr)))
                    .unwrap_or_else(|_| initial_value_for_data_type(data_type))
            }
        }
        DataType::Array2D { element_type, dims } => {
            let scalar = data_type_scalar_id(element_type);
            let values: Vec<Variant> = value
                .split(';')
                .flat_map(|row| row.split(','))
                .map(|s| string_to_variant(s.trim(), element_type, custom))
                .collect();
            Array::new_multi(scalar, values, vec![dims[0], dims[1]])
                .map(|arr| Variant::Array(Box::new(arr)))
                .unwrap_or_else(|_| initial_value_for_data_type(data_type))
        }
        DataType::Structure { name, fields } => {
            if let Some(struct_node_id) = custom.get(name) {
                let field_values: Vec<Variant> = fields
                    .iter()
                    .map(|f| {
                        let field_val = parse_struct_field_value(value, &f.name, &f.data_type, custom);
                        field_val
                    })
                    .collect();
                build_structure_variant_from_values(struct_node_id, fields, &field_values, custom)
                    .unwrap_or_else(|e| {
                        warn!("Structure variant construction failed for '{}': {}", name, e);
                        Variant::ExtensionObject(ExtensionObject::null())
                    })
            } else {
                Variant::ExtensionObject(ExtensionObject::null())
            }
        }
    }
}

fn parse_struct_field_value(
    input: &str,
    field_name: &str,
    field_dt: &DataType,
    custom: &HashMap<String, NodeId>,
) -> Variant {
    let trimmed = input.trim();
    let needle_lc = field_name.to_ascii_lowercase();
    for part in trimmed.split(',').chain(trimmed.split(';')) {
        let part = part.trim();
        if let Some((key, val)) = part.split_once('=') {
            if key.trim().to_ascii_lowercase() == needle_lc {
                return string_to_variant(val.trim(), field_dt, custom);
            }
        }
        if let Some((key, val)) = part.split_once(':') {
            if key.trim().to_ascii_lowercase() == needle_lc {
                return string_to_variant(val.trim(), field_dt, custom);
            }
        }
    }
    string_to_variant("0", field_dt, custom)
}

fn build_structure_eo(
    struct_node_id: &NodeId,
    fields: &[StructField],
    field_values: Vec<Variant>,
    custom: &HashMap<String, NodeId>,
) -> Result<Variant, String> {
    let encoding_id = derive_encoding_node_id(struct_node_id);

    let mut parent_ids = ParentIds::new();
    for builtin in [
        DataTypeId::Boolean,
        DataTypeId::Int16,
        DataTypeId::Int32,
        DataTypeId::Int64,
        DataTypeId::UInt16,
        DataTypeId::UInt32,
        DataTypeId::UInt64,
        DataTypeId::Float,
        DataTypeId::Double,
        DataTypeId::String,
        DataTypeId::DateTime,
        DataTypeId::ByteString,
    ] {
        let nid: NodeId = builtin.into();
        parent_ids.add_type(nid.clone(), nid);
    }
    for f in fields {
        register_types_recursive(&f.data_type, custom, &mut parent_ids);
    }

    let mut type_tree = DataTypeTree::new(parent_ids);

    let struct_fields: Vec<StructureField> = fields
        .iter()
        .map(|StructField { name: fname, data_type: f_dt }| {
            let field_dt = f_dt
                .register_name()
                .and_then(|n| custom.get(n))
                .cloned()
                .unwrap_or_else(|| NodeId::new(0, f_dt.type_id()));
            StructureField {
                name: UAString::from(fname.as_str()),
                data_type: field_dt,
                value_rank: -1,
                ..Default::default()
            }
        })
        .collect();

    let struct_def = DataTypeDefinition::Structure(StructureDefinition {
        default_encoding_id: encoding_id.clone(),
        base_data_type: DataTypeId::Structure.into(),
        structure_type: StructureType::Structure,
        fields: Some(struct_fields),
    });

    let type_name = if fields.is_empty() { "Empty" } else { "DynStruct" };
    let type_info = TypeInfo::from_type_definition(
        struct_def,
        type_name.to_owned(),
        Some(EncodingIds {
            binary_id: encoding_id,
            json_id: NodeId::null(),
            xml_id: NodeId::null(),
        }),
        false,
        struct_node_id,
        type_tree.parent_ids(),
    )
    .map_err(|e| format!("TypeInfo construction failed: {e:?}"))?;

    type_tree.add_type(struct_node_id.clone(), type_info);
    let type_tree = Arc::new(type_tree);

    let struct_info = type_tree
        .get_struct_type(struct_node_id)
        .ok_or("struct type not found after registration")?
        .clone();

    let dynamic = DynamicStructure::new_struct(struct_info, type_tree, field_values)
        .map_err(|e| format!("DynamicStructure::new_struct failed: {e:?}"))?;

    let eo = ExtensionObject::from_message(dynamic);
    Ok(Variant::ExtensionObject(eo))
}

fn build_structure_variant_from_values(
    struct_node_id: &NodeId,
    fields: &[StructField],
    field_values: &[Variant],
    custom: &HashMap<String, NodeId>,
) -> Result<Variant, String> {
    build_structure_eo(struct_node_id, fields, field_values.to_vec(), custom)
}

/// Format a Variant as a human-readable display string.
/// Arrays: `[1, 2, 3]`, 2D arrays: `[1,2;3,4]`, scalars: default Display.
pub fn variant_to_display_string(v: &Variant) -> String {
    match v {
        Variant::Array(arr) => {
            if let Some(dims) = &arr.dimensions {
                if dims.len() >= 2 {
                    let rows = dims[0] as usize;
                    let cols = dims[1] as usize;
                    let mut row_strs = Vec::with_capacity(rows);
                    for r in 0..rows {
                        let start = r * cols;
                        let end = start + cols;
                        let row_items: Vec<String> = arr.values[start..end.min(arr.values.len())]
                            .iter()
                            .map(|v| format!("{v}"))
                            .collect();
                        row_strs.push(row_items.join(","));
                    }
                    return format!("[{}]", row_strs.join(";"));
                }
            }
            let items: Vec<String> = arr.values.iter().map(|v| format!("{v}")).collect();
            format!("[{}]", items.join(", "))
        }
        _ => format!("{v}"),
    }
}

/// Convert an f64 value to a Variant for the given data type.
/// For complex types (Array, Array2D, Structure), generates proper values using
/// the registered `custom` type map. Enum cycles through registered fields.
pub fn f64_to_variant(value: f64, data_type: &DataType, custom: &HashMap<String, NodeId>) -> Variant {
    match data_type {
        DataType::Boolean => Variant::Boolean(value > 0.5),
        DataType::Int16 => Variant::Int16(value.clamp(i16::MIN as f64, i16::MAX as f64) as i16),
        DataType::Int32 => Variant::Int32(value.clamp(i32::MIN as f64, i32::MAX as f64) as i32),
        DataType::Int64 => Variant::Int64(value.clamp(i64::MIN as f64, i64::MAX as f64) as i64),
        DataType::UInt16 => Variant::UInt16(value.clamp(0.0, u16::MAX as f64) as u16),
        DataType::UInt32 => Variant::UInt32(value.clamp(0.0, u32::MAX as f64) as u32),
        DataType::UInt64 => Variant::UInt64(value.clamp(0.0, u64::MAX as f64) as u64),
        DataType::Float => Variant::Float(value as f32),
        DataType::Double => Variant::Double(value),
        DataType::String => Variant::String(UAString::from(format!("{:.2}", value))),
        DataType::DateTime => Variant::Double(value),
        DataType::ByteString => Variant::Double(value),
        DataType::Enum { fields, .. } => {
            if fields.is_empty() {
                return Variant::Int32(0);
            }
            let idx = ((value.round() as i64).rem_euclid(fields.len() as i64)) as usize;
            let v = fields.get(idx).map(|(v, _)| *v).unwrap_or(0);
            Variant::Int32(v as i32)
        }
        DataType::Array { element_type } => {
            let scalar = data_type_scalar_id(element_type);
            let values: Vec<Variant> = (0..DEFAULT_ARRAY_LEN)
                .map(|i| f64_to_variant(value + i as f64, element_type, custom))
                .collect();
            Array::new(scalar, values)
                .map(|arr| Variant::Array(Box::new(arr)))
                .unwrap_or_else(|_| initial_value_for_data_type(data_type))
        }
        DataType::Array2D { element_type, dims } => {
            let scalar = data_type_scalar_id(element_type);
            let count = (dims[0] as usize) * (dims[1] as usize);
            let values: Vec<Variant> = (0..count)
                .map(|i| f64_to_variant(value + i as f64, element_type, custom))
                .collect();
            Array::new_multi(scalar, values, vec![dims[0], dims[1]])
                .map(|arr| Variant::Array(Box::new(arr)))
                .unwrap_or_else(|_| initial_value_for_data_type(data_type))
        }
        DataType::Structure { name, fields } => {
            if let Some(struct_node_id) = custom.get(name) {
                build_structure_variant(struct_node_id, fields, value, custom)
                    .unwrap_or_else(|e| {
                        warn!("Structure variant construction failed for '{}': {}", name, e);
                        Variant::ExtensionObject(ExtensionObject::null())
                    })
            } else {
                Variant::ExtensionObject(ExtensionObject::null())
            }
        }
    }
}

fn derive_encoding_node_id(type_node_id: &NodeId) -> NodeId {
    match &type_node_id.identifier {
        Identifier::String(s) => {
            let type_str = s.value().as_deref().unwrap_or_default();
            NodeId::new(type_node_id.namespace, format!("{}_be", type_str))
        }
        _ => NodeId::null(),
    }
}

fn register_types_recursive(
    dt: &DataType,
    custom: &HashMap<String, NodeId>,
    parent_ids: &mut ParentIds,
) {
    let self_id = dt.type_node_id(custom);
    let parent_id: NodeId = match dt {
        DataType::Structure { fields, .. } => {
            for f in fields {
                register_types_recursive(&f.data_type, custom, parent_ids);
            }
            DataTypeId::Structure.into()
        }
        DataType::Enum { .. } => DataTypeId::Enumeration.into(),
        _ => self_id.clone(),
    };
    parent_ids.add_type(self_id, parent_id);
}

fn build_structure_variant(
    struct_node_id: &NodeId,
    fields: &[StructField],
    seed_value: f64,
    custom: &HashMap<String, NodeId>,
) -> Result<Variant, String> {
    let field_values: Vec<Variant> = fields
        .iter()
        .enumerate()
        .map(|(i, f)| f64_to_variant(seed_value + (i as f64) * 0.1, &f.data_type, custom))
        .collect();
    build_structure_eo(struct_node_id, fields, field_values, custom)
}

/// Parse a node_id string to OPC UA NodeId.
pub fn parse_node_id(node_id_str: &str) -> Result<NodeId, OpcUaSimError> {
    node_id_str.parse::<NodeId>().map_err(|e| {
        OpcUaSimError::ServerError(format!("Invalid node id '{}': {}", node_id_str, e))
    })
}

/// Populate an address space with folders and variable nodes.
pub fn populate_address_space(
    address_space: &mut AddressSpace,
    namespace_index: u16,
    folders: &[ServerFolder],
    nodes: &[ServerNode],
    custom: &HashMap<String, NodeId>,
) {
    for folder in folders {
        let node_id = make_node_id(namespace_index, &folder.node_id);
        let parent_id = make_parent_id(namespace_index, &folder.parent_id);
        address_space.add_folder(
            &node_id,
            QualifiedName::new(namespace_index, &folder.display_name),
            LocalizedText::new("", &folder.display_name),
            &parent_id,
        );
    }

    for node in nodes {
        add_variable_node(address_space, namespace_index, node, custom);
    }
}

/// Add a single variable node to the address space.
pub fn add_variable_node(
    address_space: &mut AddressSpace,
    namespace_index: u16,
    node: &ServerNode,
    custom: &HashMap<String, NodeId>,
) -> bool {
    let node_id = make_node_id(namespace_index, &node.node_id);
    let parent_id = make_parent_id(namespace_index, &node.parent_id);
    if node.data_type.is_custom() && node.data_type.register_name().and_then(|n| custom.get(n)).is_none() {
        warn!(
            "Skipping variable '{}': custom type {:?} not registered",
            node.display_name, node.data_type
        );
        return false;
    }
    if node.data_type.has_custom_element() {
        warn!(
            "Skipping variable '{}': arrays of custom element types are not supported",
            node.display_name
        );
        return false;
    }
    let dt_node_id = data_type_to_node_id(&node.data_type, custom);

    let initial_value = match &node.simulation {
        SimulationMode::Static { value } => string_to_variant(value, &node.data_type, custom),
        _ => string_to_variant("0", &node.data_type, custom),
    };

    let mut builder = VariableBuilder::new(&node_id, &node.display_name, &node.display_name)
        .data_type(dt_node_id)
        .value(initial_value)
        .organized_by(parent_id)
        .history_readable();

    if node.writable {
        builder = builder.writable();
    }

    let inserted = builder.insert(address_space);
    if inserted && node.data_type.is_numeric() {
        add_eu_range_property(address_space, namespace_index, node);
    }
    inserted
}

/// Add the EURange property (array [low, high] of Double) to a variable node.
/// Percent deadband filtering requires this property (OPC UA Part 4 7.17.4).
fn add_eu_range_property(
    address_space: &mut AddressSpace,
    namespace_index: u16,
    node: &ServerNode,
) {
    let var_id = make_node_id(namespace_index, &node.node_id);
    let prop_id = NodeId::new(namespace_index, format!("{}_EURange", node.node_id));
    let prop = VariableBuilder::new(&prop_id, "EURange", "EURange")
        .data_type(DataTypeId::Double)
        .value(Variant::from(vec![node.eu_range_low, node.eu_range_high]))
        .value_rank(1)
        .build();
    address_space.insert(
        prop,
        Some(&[(
            &var_id,
            &ReferenceTypeId::HasProperty,
            ReferenceDirection::Inverse,
        )]),
    );
}

/// Remove a node from the address space.
pub fn remove_node(
    address_space: &mut AddressSpace,
    namespace_index: u16,
    node_id_str: &str,
) -> bool {
    let node_id = make_node_id(namespace_index, node_id_str);
    address_space.delete(&node_id, true).is_some()
}

/// Create an OPC UA NodeId from a string, handling namespace prefixed formats.
fn make_node_id(namespace_index: u16, id_str: &str) -> NodeId {
    // If it already has a namespace prefix (ns=X;), parse directly
    if id_str.starts_with("ns=") || id_str.starts_with("i=") || id_str.starts_with("s=") {
        id_str
            .parse::<NodeId>()
            .unwrap_or_else(|_| NodeId::new(namespace_index, id_str))
    } else {
        NodeId::new(namespace_index, id_str)
    }
}

/// Resolve parent_id: "i=85" is the Objects folder (root).
fn make_parent_id(namespace_index: u16, parent_id: &str) -> NodeId {
    if parent_id == "i=85" || parent_id.is_empty() {
        NodeId::objects_folder_id()
    } else {
        make_node_id(namespace_index, parent_id)
    }
}

/// Collect the unique `Enum`/`Structure` data types referenced by the supplied
/// server nodes, preserving first-seen order. Entries with a duplicate name
/// but a different definition are skipped (a warning is logged).
pub fn collect_custom_data_types(nodes: &[ServerNode]) -> Vec<DataType> {
    let mut seen_names: HashSet<String> = HashSet::new();
    let mut out: Vec<DataType> = Vec::new();

    for node in nodes {
        collect_recursive(&node.data_type, &mut seen_names, &mut out);
    }
    out
}

fn collect_recursive(
    dt: &DataType,
    seen_names: &mut HashSet<String>,
    out: &mut Vec<DataType>,
) {
    match dt {
        DataType::Enum { name, .. } | DataType::Structure { name, .. } => {
            if seen_names.insert(name.clone()) {
                match dt {
                    DataType::Structure { fields, .. } => {
                        for f in fields {
                            collect_recursive(&f.data_type, seen_names, out);
                        }
                        out.push(dt.clone());
                    }
                    DataType::Enum { .. } => {
                        out.push(dt.clone());
                    }
                    _ => unreachable!(),
                }
            } else {
                warn!("Duplicate custom data type name '{}' skipped", name);
            }
        }
        DataType::Array { element_type } | DataType::Array2D { element_type, .. } => {
            if element_type.is_custom() {
                warn!(
                    "Arrays of custom element type ({:?}) are not supported by Task 6",
                    element_type
                );
            }
        }
        _ => {}
    }
}

/// Register the collected custom DataType nodes (and, for `Structure`s, the
/// companion `DataTypeEncodingType` object nodes) in the supplied address
/// space. The resulting `HashMap` maps registered type *names* (the
/// discriminator key used by [`DataType::type_node_id`]) to their
/// namespace-scoped NodeIds.
pub fn register_custom_types_in_address_space(
    address_space: &mut AddressSpace,
    namespace_index: u16,
    nodes: &[ServerNode],
) -> HashMap<String, NodeId> {
    let collected = collect_custom_data_types(nodes);
    let mut map: HashMap<String, NodeId> = HashMap::new();

    for (idx, dt) in collected.iter().enumerate() {
        let type_node_id = NodeId::new(namespace_index, format!("type_{}", idx));
        let name = match dt.register_name() {
            Some(n) => n.to_string(),
            None => continue,
        };
        match dt {
            DataType::Enum { fields, .. } => {
                let enum_fields: Vec<EnumField> = fields
                    .iter()
                    .map(|(value, display_name)| EnumField {
                        value: *value,
                        name: UAString::from(display_name.as_str()),
                        display_name: LocalizedText::new("", display_name),
                        ..Default::default()
                    })
                    .collect();
                let def = DataTypeDefinition::Enum(EnumDefinition {
                    fields: Some(enum_fields),
                });
                DataTypeBuilder::new(&type_node_id, &name, &name)
                    .data_type_definition(def)
                    .is_abstract(false)
                    .subtype_of(DataTypeId::Enumeration)
                    .insert(address_space);
            }
            DataType::Structure { fields, .. } => {
                let encoding_node_id =
                    NodeId::new(namespace_index, format!("type_{}_be", idx));
                let struct_fields: Vec<StructureField> = fields
                    .iter()
                    .map(|StructField { name: fname, data_type: f_dt }| {
                        let field_dt = if let Some(registered) =
                            f_dt.register_name().and_then(|n| map.get(n))
                        {
                            registered.clone()
                        } else {
                            NodeId::new(0, f_dt.type_id())
                        };
                        StructureField {
                            name: UAString::from(fname.as_str()),
                            data_type: field_dt,
                            value_rank: -1,
                            ..Default::default()
                        }
                    })
                    .collect();
                let def = DataTypeDefinition::Structure(StructureDefinition {
                    default_encoding_id: encoding_node_id.clone(),
                    base_data_type: DataTypeId::Structure.into(),
                    structure_type: StructureType::Structure,
                    fields: Some(struct_fields),
                });
                DataTypeBuilder::new(&type_node_id, &name, &name)
                    .data_type_definition(def)
                    .is_abstract(false)
                    .subtype_of(DataTypeId::Structure)
                    .reference(&encoding_node_id, ReferenceTypeId::HasEncoding, ReferenceDirection::Forward)
                    .insert(address_space);
                ObjectBuilder::new(&encoding_node_id, "Default Binary", "Default Binary")
                    .has_type_definition(ObjectTypeId::DataTypeEncodingType)
                    .reference(&type_node_id, ReferenceTypeId::HasEncoding, ReferenceDirection::Inverse)
                    .insert(address_space);
            }
            _ => continue,
        }
        map.insert(name, type_node_id);
    }

    info!("Registered {} custom type(s) in address space", map.len());
    map
}

/// Register the same custom types in the server-level [`DefaultTypeTree`]
/// (accessible via [`opcua_server::ServerHandle::type_tree`]). This is the
/// structure the server uses for browse filtering and event filters. The
/// function is deliberately safe to call after `server.run()` has populated
/// the tree's core namespaces.
pub fn register_custom_types_in_type_tree(
    type_tree: &mut DefaultTypeTree,
    custom: &HashMap<String, NodeId>,
    nodes: &[ServerNode],
) {
    let mut counter = 0usize;
    let mut seen: HashSet<String> = HashSet::new();

    fn visit(
        dt: &DataType,
        counter: &mut usize,
        seen: &mut HashSet<String>,
        custom: &HashMap<String, NodeId>,
        type_tree: &mut DefaultTypeTree,
    ) {
        match dt {
            DataType::Structure { name, fields, .. } => {
                if seen.insert(name.clone()) {
                    for f in fields {
                        visit(&f.data_type, counter, seen, custom, type_tree);
                    }
                    if let Some(type_node_id) = custom.get(name) {
                        let parent: NodeId = DataTypeId::Structure.into();
                        type_tree.add_type_node(type_node_id, &parent, NodeClass::DataType);
                    }
                    *counter += 1;
                }
            }
            DataType::Enum { name, .. } => {
                if seen.insert(name.clone()) {
                    if let Some(type_node_id) = custom.get(name) {
                        let parent: NodeId = DataTypeId::Enumeration.into();
                        type_tree.add_type_node(type_node_id, &parent, NodeClass::DataType);
                    }
                    *counter += 1;
                }
            }
            _ => {}
        }
    }

    for node in nodes {
        visit(
            &node.data_type,
            &mut counter,
            &mut seen,
            custom,
            type_tree,
        );
    }
    info!(
        "Registered {} custom type node(s) in DefaultTypeTree",
        counter
    );
}

#[cfg(test)]
mod encoding_verification_tests {
    use std::sync::Arc;

    use super::*;
    use opcua_types::{
        custom::{DataTypeTree, EncodingIds, ParentIds, StructTypeInfo, TypeInfo},
        ContextOwned, DecodingOptions, NamespaceMap, NodeId, StructureDefinition, StructureField,
        StructureType, TypeLoaderCollection, Variant,
    };

    #[test]
    fn structure_encoding_via_extension_object() {
        let mut parent_ids = ParentIds::new();
        parent_ids.add_type(DataTypeId::Int32.into(), DataTypeId::Int32.into());
        parent_ids.add_type(DataTypeId::Double.into(), DataTypeId::Double.into());

        let struct_node_id = NodeId::new(2, 5);
        let binary_encoding_id = NodeId::new(2, 6);

        parent_ids.add_type(struct_node_id.clone(), DataTypeId::Structure.into());

        let mut type_tree = DataTypeTree::new(parent_ids);
        let struct_def = DataTypeDefinition::Structure(StructureDefinition {
            default_encoding_id: binary_encoding_id.clone(),
            base_data_type: DataTypeId::Structure.into(),
            structure_type: StructureType::Structure,
            fields: Some(vec![
                StructureField {
                    name: UAString::from("A"),
                    data_type: DataTypeId::Int32.into(),
                    value_rank: -1,
                    ..Default::default()
                },
                StructureField {
                    name: UAString::from("B"),
                    data_type: DataTypeId::Double.into(),
                    value_rank: -1,
                    ..Default::default()
                },
            ]),
        });
        let type_info = TypeInfo::from_type_definition(
            struct_def,
            "Sample".to_owned(),
            Some(EncodingIds {
                binary_id: binary_encoding_id.clone(),
                json_id: NodeId::null(),
                xml_id: NodeId::null(),
            }),
            false,
            &struct_node_id,
            type_tree.parent_ids(),
        )
        .expect("type info from definition");
        type_tree.add_type(struct_node_id.clone(), type_info);
        let type_tree = Arc::new(type_tree);

        let struct_info = type_tree
            .get_struct_type(&struct_node_id)
            .expect("struct type is registered")
            .clone();
        let dynamic = opcua_types::custom::DynamicStructure::new_struct(
            struct_info,
            type_tree.clone(),
            vec![Variant::Int32(7), Variant::Double(3.5)],
        )
        .expect("new_struct validates field count + ordering");

        let eo = ExtensionObject::from_message(dynamic);
        assert_eq!(
            eo.binary_type_id().node_id,
            binary_encoding_id,
            "Encoding ID must match the registered binary_id, otherwise DynamicStructure encoding path is broken",
        );

        let loader = opcua_types::custom::DynamicTypeLoader::new(type_tree.clone());
        let mut loaders = TypeLoaderCollection::new_empty();
        loaders.add_type_loader(loader);
        let ctx = ContextOwned::new(NamespaceMap::new(), loaders, DecodingOptions::test());
        let byte_len = opcua_types::BinaryEncodable::byte_len(&eo, &ctx.context());
        assert!(byte_len > 0, "encoded ExtensionObject must have non-zero length");
    }
}
