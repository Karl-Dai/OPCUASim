//! Lightweight ContentFilter evaluator (OPC UA Part 4 filtering, event field
//! array operations).

use std::cmp::Ordering;

use opcua_types::{ContentFilterElement, ExtensionObject, FilterOperator, StatusCode, Variant};

/// Event field names used both by the filter evaluator and by
/// [`crate::server::history_node_manager`].
pub const EVENT_FIELD_NAMES: &[&str] = &[
    "EventId",
    "EventType",
    "SourceNode",
    "SourceName",
    "Time",
    "ReceiveTime",
    "Message",
    "Severity",
];

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Evaluate a where-clause against an event field array.
///
/// An empty or missing element list means "select everything" (returns `Ok(true)`).
/// Multiple elements are implicitly ANDed — every element must evaluate to `true`
/// for the clause to pass.
pub fn eval_clauses(
    elements: &[ContentFilterElement],
    fields: &[Variant],
) -> Result<bool, StatusCode> {
    if elements.is_empty() {
        return Ok(true);
    }
    for el in elements {
        if !eval_element(el, fields)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Recursively evaluate a single [`ContentFilterElement`].
fn eval_element(el: &ContentFilterElement, fields: &[Variant]) -> Result<bool, StatusCode> {
    let operands = el.filter_operands.as_deref().unwrap_or(&[]);
    let op = el.filter_operator;

    match op {
        // Unary operators
        FilterOperator::IsNull => {
            let v = resolve_operand(operands, 0, fields)?;
            Ok(matches!(v, Variant::Empty))
        }
        FilterOperator::Not => {
            let v = resolve_operand(operands, 0, fields)?;
            let b = variant_to_bool(&v)?;
            Ok(!b)
        }

        // Binary comparison operators
        FilterOperator::Equals
        | FilterOperator::GreaterThan
        | FilterOperator::LessThan
        | FilterOperator::GreaterThanOrEqual
        | FilterOperator::LessThanOrEqual => {
            let a = resolve_operand(operands, 0, fields)?;
            let b = resolve_operand(operands, 1, fields)?;
            compare(&a, &b, op)
        }

        // Like — string pattern matching
        FilterOperator::Like => {
            let field_val = resolve_operand(operands, 0, fields)?;
            let pat_val = resolve_operand(operands, 1, fields)?;
            let s = variant_to_string(&field_val)?;
            let pat = variant_to_string(&pat_val)?;
            Ok(like_match(&s, &pat))
        }

        // And / Or — logical on two sub-expressions
        FilterOperator::And => {
            let a = resolve_bool_operand(operands, 0, fields)?;
            let b = resolve_bool_operand(operands, 1, fields)?;
            Ok(a && b)
        }
        FilterOperator::Or => {
            let a = resolve_bool_operand(operands, 0, fields)?;
            let b = resolve_bool_operand(operands, 1, fields)?;
            Ok(a || b)
        }

        // Between — field between lower and upper (inclusive)
        FilterOperator::Between => {
            if operands.len() < 3 {
                return Err(StatusCode::BadFilterOperandInvalid);
            }
            let field_val = resolve_operand(operands, 0, fields)?;
            let lower = resolve_operand(operands, 1, fields)?;
            let upper = resolve_operand(operands, 2, fields)?;
            let ge_lower = compare(&field_val, &lower, FilterOperator::GreaterThanOrEqual)?;
            let le_upper = compare(&field_val, &upper, FilterOperator::LessThanOrEqual)?;
            Ok(ge_lower && le_upper)
        }

        // InList — field equals any in list
        FilterOperator::InList => {
            if operands.len() < 2 {
                return Err(StatusCode::BadFilterOperandInvalid);
            }
            let field_val = resolve_operand(operands, 0, fields)?;
            for operand in &operands[1..] {
                let candidate = operand_variant(operand, fields)?;
                if compare(&field_val, &candidate, FilterOperator::Equals)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }

        // Unsupported operators
        FilterOperator::Cast
        | FilterOperator::InView
        | FilterOperator::OfType
        | FilterOperator::RelatedTo
        | FilterOperator::BitwiseAnd
        | FilterOperator::BitwiseOr => Err(StatusCode::BadFilterOperatorInvalid),
    }
}

/// Compare two variants using the given comparison operator.
pub fn compare(a: &Variant, b: &Variant, op: FilterOperator) -> Result<bool, StatusCode> {
    let ordering = compare_variants(a, b)?;

    match op {
        FilterOperator::Equals => Ok(ordering == Ordering::Equal),
        FilterOperator::GreaterThan => Ok(ordering == Ordering::Greater),
        FilterOperator::LessThan => Ok(ordering == Ordering::Less),
        FilterOperator::GreaterThanOrEqual => Ok(ordering != Ordering::Less),
        FilterOperator::LessThanOrEqual => Ok(ordering != Ordering::Greater),
        _ => Err(StatusCode::BadFilterOperatorInvalid),
    }
}

fn compare_variants(a: &Variant, b: &Variant) -> Result<Ordering, StatusCode> {
    // Both numeric → compare as f64.
    if let (Some(af), Some(bf)) = (variant_to_f64(a), variant_to_f64(b)) {
        return Ok(af.partial_cmp(&bf).unwrap_or(Ordering::Equal));
    }
    // Both boolean → compare as bool.
    if let (Variant::Boolean(ab), Variant::Boolean(bb)) = (a, b) {
        return Ok(ab.cmp(bb));
    }
    // Both string → compare as string.
    if let (Variant::String(as_), Variant::String(bs)) = (a, b) {
        return Ok(as_.as_ref().cmp(bs.as_ref()));
    }
    // Mixed types → fallback string comparison.
    let sa = format!("{a}");
    let sb = format!("{b}");
    Ok(sa.cmp(&sb))
}

/// Simple LIKE wildcard matching: `%` matches any number of characters,
/// `_` matches exactly one character.
pub fn like_match(s: &str, pat: &str) -> bool {
    let s_bytes = s.as_bytes();
    let pat_bytes = pat.as_bytes();
    let n = s_bytes.len();
    let m = pat_bytes.len();

    // DP table: dp[i][j] = pattern[..i] matches string[..j]
    let mut dp = vec![vec![false; n + 1]; m + 1];
    dp[0][0] = true;

    // Handle leading '%' — they can match empty string.
    for i in 1..=m {
        if pat_bytes[i - 1] == b'%' {
            dp[i][0] = dp[i - 1][0];
        } else {
            break;
        }
    }

    for i in 1..=m {
        for j in 1..=n {
            match pat_bytes[i - 1] {
                b'%' => {
                    // % matches zero or more characters.
                    dp[i][j] = dp[i - 1][j] || dp[i][j - 1];
                }
                b'_' => {
                    // _ matches exactly one character.
                    dp[i][j] = dp[i - 1][j - 1];
                }
                pc => {
                    dp[i][j] = dp[i - 1][j - 1] && s_bytes[j - 1] == pc;
                }
            }
        }
    }

    dp[m][n]
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Resolve an [`ExtensionObject`] operand to a [`Variant`].
///
/// - [`SimpleAttributeOperand`] → field value (by browse-path name → index).
/// - [`LiteralOperand`] → literal value.
/// - [`ContentFilterElement`] → recursive evaluation returning a boolean.
/// - Unrecognised → `BadFilterOperandInvalid`.
pub(crate) fn operand_variant(
    operand: &ExtensionObject,
    fields: &[Variant],
) -> Result<Variant, StatusCode> {
    // Try SimpleAttributeOperand → field value by name.
    if let Some(sao) = operand.inner_as::<opcua_types::SimpleAttributeOperand>() {
        let field_name = sao
            .browse_path
            .as_ref()
            .and_then(|path| path.last().map(|qn| qn.name.to_string()));
        match field_name {
            Some(name) => {
                let idx = EVENT_FIELD_NAMES
                    .iter()
                    .position(|n| *n == name)
                    .ok_or(StatusCode::BadFilterOperandInvalid)?;
                Ok(fields.get(idx).cloned().unwrap_or(Variant::Empty))
            }
            None => Err(StatusCode::BadFilterOperandInvalid),
        }
    }
    // Try LiteralOperand → literal value.
    else if let Some(lit) = operand.inner_as::<opcua_types::LiteralOperand>() {
        Ok(lit.value.clone())
    }
    // Try ContentFilterElement → recursive evaluation (returns boolean).
    else if let Some(sub) = operand.inner_as::<ContentFilterElement>() {
        eval_element(sub, fields).map(Variant::Boolean)
    } else {
        Err(StatusCode::BadFilterOperandInvalid)
    }
}

/// Resolve and validate that the operand at `index` is present.
fn resolve_operand(
    operands: &[ExtensionObject],
    index: usize,
    fields: &[Variant],
) -> Result<Variant, StatusCode> {
    operands
        .get(index)
        .ok_or(StatusCode::BadFilterOperandInvalid)
        .and_then(|eo| operand_variant(eo, fields))
}

/// Resolve a boolean operand (for And/Or sub-expressions).
fn resolve_bool_operand(
    operands: &[ExtensionObject],
    index: usize,
    fields: &[Variant],
) -> Result<bool, StatusCode> {
    let v = resolve_operand(operands, index, fields)?;
    variant_to_bool(&v)
}

fn variant_to_bool(v: &Variant) -> Result<bool, StatusCode> {
    match v {
        Variant::Boolean(b) => Ok(*b),
        Variant::Empty => Ok(false),
        _ => Err(StatusCode::BadFilterOperandInvalid),
    }
}

fn variant_to_string(v: &Variant) -> Result<String, StatusCode> {
    match v {
        Variant::String(s) => Ok(s.as_ref().to_string()),
        Variant::Empty => Ok(String::new()),
        _ => Err(StatusCode::BadFilterOperandInvalid),
    }
}

fn variant_to_f64(v: &Variant) -> Option<f64> {
    match v {
        Variant::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
        Variant::SByte(x) => Some(*x as f64),
        Variant::Byte(x) => Some(*x as f64),
        Variant::Int16(x) => Some(*x as f64),
        Variant::UInt16(x) => Some(*x as f64),
        Variant::Int32(x) => Some(*x as f64),
        Variant::UInt32(x) => Some(*x as f64),
        Variant::Int64(x) => Some(*x as f64),
        Variant::UInt64(x) => Some(*x as f64),
        Variant::Float(x) => Some(*x as f64),
        Variant::Double(x) => Some(*x),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use opcua_types::{
        AttributeId, ContentFilterElement, ExtensionObject, FilterOperator, LiteralOperand, NodeId,
        NumericRange, QualifiedName, SimpleAttributeOperand, Variant,
    };

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    fn sao(field_name: &str) -> ExtensionObject {
        ExtensionObject::from_message(SimpleAttributeOperand {
            type_definition_id: NodeId::null(),
            browse_path: Some(vec![QualifiedName::from(field_name)]),
            attribute_id: AttributeId::Value as u32,
            index_range: NumericRange::None,
        })
    }

    fn lit(v: Variant) -> ExtensionObject {
        ExtensionObject::from_message(LiteralOperand { value: v })
    }

    fn sub_el(op: FilterOperator, operands: Vec<ExtensionObject>) -> ExtensionObject {
        ExtensionObject::from_message(ContentFilterElement {
            filter_operator: op,
            filter_operands: Some(operands),
        })
    }

    fn el(op: FilterOperator, operands: Vec<ExtensionObject>) -> ContentFilterElement {
        ContentFilterElement {
            filter_operator: op,
            filter_operands: Some(operands),
        }
    }

    fn event_fields() -> Vec<Variant> {
        vec![
            Variant::String("evt-001".into()),       // EventId
            Variant::String("BaseEventType".into()), // EventType
            Variant::String("ns=2;s=Demo".into()),   // SourceNode
            Variant::String("DemoSource".into()),    // SourceName
            Variant::String("2025-01-01".into()),    // Time
            Variant::String("2025-01-01".into()),    // ReceiveTime
            Variant::String("hello world".into()),   // Message
            Variant::UInt16(500),                    // Severity
        ]
    }

    // ------------------------------------------------------------------
    // operand_variant
    // ------------------------------------------------------------------

    #[test]
    fn operand_variant_known_field() {
        let fields = event_fields();
        let v = operand_variant(&sao("Message"), &fields).unwrap();
        assert_eq!(v, Variant::String("hello world".into()));
    }

    #[test]
    fn operand_variant_unknown_field() {
        let fields = event_fields();
        let result = operand_variant(&sao("NoSuchField"), &fields);
        assert!(result.is_err());
    }

    #[test]
    fn operand_variant_literal() {
        let fields = event_fields();
        let v = operand_variant(&lit(Variant::Int32(42)), &fields).unwrap();
        assert_eq!(v, Variant::Int32(42));
    }

    // ------------------------------------------------------------------
    // eval_clauses — empty
    // ------------------------------------------------------------------

    #[test]
    fn eval_clauses_empty_elements() {
        let fields = event_fields();
        assert!(eval_clauses(&[], &fields).unwrap());
    }

    // ------------------------------------------------------------------
    // Equals
    // ------------------------------------------------------------------

    #[test]
    fn equals_string_match() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::Equals,
            vec![sao("Message"), lit(Variant::String("hello world".into()))],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn equals_string_no_match() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::Equals,
            vec![sao("Message"), lit(Variant::String("other".into()))],
        )];
        assert!(!eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn equals_numeric() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::Equals,
            vec![sao("Severity"), lit(Variant::UInt16(500))],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn equals_boolean() {
        let fields = vec![Variant::Boolean(true)];
        let clause = vec![el(
            FilterOperator::Equals,
            vec![sao("EventId"), lit(Variant::Boolean(true))],
        )];
        // EventId field is Bool(true), sao("EventId") resolves to field[0] = Bool(true)
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    // ------------------------------------------------------------------
    // GT / LT / GTE / LTE
    // ------------------------------------------------------------------

    #[test]
    fn greater_than_true() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::GreaterThan,
            vec![sao("Severity"), lit(Variant::UInt16(100))],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn greater_than_false() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::GreaterThan,
            vec![sao("Severity"), lit(Variant::UInt16(900))],
        )];
        assert!(!eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn less_than_true() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::LessThan,
            vec![sao("Severity"), lit(Variant::UInt16(900))],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn less_than_false() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::LessThan,
            vec![sao("Severity"), lit(Variant::UInt16(100))],
        )];
        assert!(!eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn gte_true() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::GreaterThanOrEqual,
            vec![sao("Severity"), lit(Variant::UInt16(500))],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn lte_true() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::LessThanOrEqual,
            vec![sao("Severity"), lit(Variant::UInt16(500))],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    // ------------------------------------------------------------------
    // Cross-type comparison (Int32 vs Double)
    // ------------------------------------------------------------------

    #[test]
    fn cross_type_int32_double() {
        let fields = vec![Variant::Int32(10)];
        let clause = vec![el(
            FilterOperator::GreaterThan,
            vec![sao("EventId"), lit(Variant::Double(5.0))],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    // ------------------------------------------------------------------
    // Not
    // ------------------------------------------------------------------

    #[test]
    fn not_operator() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::Not,
            vec![sub_el(
                FilterOperator::Equals,
                vec![sao("Message"), lit(Variant::String("hello world".into()))],
            )],
        )];
        assert!(!eval_clauses(&clause, &fields).unwrap());
    }

    // ------------------------------------------------------------------
    // And / Or
    // ------------------------------------------------------------------

    #[test]
    fn and_operator() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::And,
            vec![
                sub_el(
                    FilterOperator::Equals,
                    vec![sao("Message"), lit(Variant::String("hello world".into()))],
                ),
                sub_el(
                    FilterOperator::Equals,
                    vec![sao("Severity"), lit(Variant::UInt16(500))],
                ),
            ],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn or_operator() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::Or,
            vec![
                sub_el(
                    FilterOperator::Equals,
                    vec![sao("Message"), lit(Variant::String("hello world".into()))],
                ),
                sub_el(
                    FilterOperator::Equals,
                    vec![sao("Severity"), lit(Variant::UInt16(999))],
                ),
            ],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    // ------------------------------------------------------------------
    // Between
    // ------------------------------------------------------------------

    #[test]
    fn between_operator() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::Between,
            vec![
                sao("Severity"),
                lit(Variant::UInt16(100)),
                lit(Variant::UInt16(900)),
            ],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    // ------------------------------------------------------------------
    // InList
    // ------------------------------------------------------------------

    #[test]
    fn in_list_operator() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::InList,
            vec![
                sao("Severity"),
                lit(Variant::UInt16(100)),
                lit(Variant::UInt16(500)),
                lit(Variant::UInt16(900)),
            ],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn in_list_no_match() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::InList,
            vec![
                sao("Severity"),
                lit(Variant::UInt16(100)),
                lit(Variant::UInt16(200)),
            ],
        )];
        assert!(!eval_clauses(&clause, &fields).unwrap());
    }

    // ------------------------------------------------------------------
    // Like
    // ------------------------------------------------------------------

    #[test]
    fn like_match_percent() {
        assert!(like_match("hello world", "hello%"));
        assert!(like_match("hello world", "%world"));
        assert!(like_match("hello world", "%lo wo%"));
        assert!(!like_match("hello world", "xyz%"));
    }

    #[test]
    fn like_match_underscore() {
        assert!(like_match("cat", "c_t"));
        assert!(like_match("cat", "_a_"));
        assert!(!like_match("cat", "c__t"));
        assert!(like_match("abc", "a_c"));
    }

    #[test]
    fn like_operator_in_filter() {
        let fields = event_fields();
        let clause = vec![el(
            FilterOperator::Like,
            vec![sao("Message"), lit(Variant::String("hello%".into()))],
        )];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    // ------------------------------------------------------------------
    // IsNull
    // ------------------------------------------------------------------

    #[test]
    fn is_null_empty_variant() {
        let fields = vec![Variant::Empty];
        let clause = vec![el(FilterOperator::IsNull, vec![sao("EventId")])];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn is_null_non_empty() {
        let fields = event_fields();
        let clause = vec![el(FilterOperator::IsNull, vec![sao("EventId")])];
        assert!(!eval_clauses(&clause, &fields).unwrap());
    }

    // ------------------------------------------------------------------
    // Multi-element implicit AND
    // ------------------------------------------------------------------

    #[test]
    fn multi_element_implicit_and() {
        let fields = event_fields();
        let clause = vec![
            el(
                FilterOperator::Equals,
                vec![
                    sao("EventType"),
                    lit(Variant::String("BaseEventType".into())),
                ],
            ),
            el(
                FilterOperator::GreaterThan,
                vec![sao("Severity"), lit(Variant::UInt16(100))],
            ),
        ];
        assert!(eval_clauses(&clause, &fields).unwrap());
    }

    #[test]
    fn multi_element_implicit_and_one_false() {
        let fields = event_fields();
        let clause = vec![
            el(
                FilterOperator::Equals,
                vec![
                    sao("EventType"),
                    lit(Variant::String("BaseEventType".into())),
                ],
            ),
            el(
                FilterOperator::GreaterThan,
                vec![sao("Severity"), lit(Variant::UInt16(900))],
            ),
        ];
        assert!(!eval_clauses(&clause, &fields).unwrap());
    }

    // ------------------------------------------------------------------
    // Error cases
    // ------------------------------------------------------------------

    #[test]
    fn cast_operator_returns_error() {
        let fields = event_fields();
        let clause = vec![el(FilterOperator::Cast, vec![sao("Severity")])];
        assert!(eval_clauses(&clause, &fields).is_err());
    }

    #[test]
    fn unsupported_operator_returns_error() {
        let fields = event_fields();
        let clause = vec![el(FilterOperator::BitwiseAnd, vec![sao("Severity")])];
        assert!(eval_clauses(&clause, &fields).is_err());
    }

    // ------------------------------------------------------------------
    // compare() unit tests
    // ------------------------------------------------------------------

    #[test]
    fn compare_string_equals() {
        assert!(compare(
            &Variant::String("abc".into()),
            &Variant::String("abc".into()),
            FilterOperator::Equals,
        )
        .unwrap());
    }

    #[test]
    fn compare_string_not_equals() {
        assert!(!compare(
            &Variant::String("abc".into()),
            &Variant::String("xyz".into()),
            FilterOperator::Equals,
        )
        .unwrap());
    }

    #[test]
    fn compare_numeric_f64() {
        assert!(compare(
            &Variant::Int32(10),
            &Variant::Double(5.0),
            FilterOperator::GreaterThan,
        )
        .unwrap());
    }

    #[test]
    fn compare_boolean() {
        assert!(compare(
            &Variant::Boolean(true),
            &Variant::Boolean(true),
            FilterOperator::Equals,
        )
        .unwrap());
    }
}
