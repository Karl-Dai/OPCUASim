//! 聚合函数计算(OPC UA Part 11 Aggregates 简化实现)
//!
//! 将时间区间内的样本按 processing_interval 切分成等长桶,
//! 对每个桶计算指定的聚合函数,返回桶序列 DataValue。

use chrono::Duration as ChronoDuration;
use opcua_types::{DataValue, DateTime, NodeId, ObjectId, StatusCode, Variant};

// ---------------------------------------------------------------------------
// Variant → f64 转换
// ---------------------------------------------------------------------------

/// 将 Variant 转换为 f64,不可转换的类型返回 None。
/// 与 `crate::history::variant_to_f64` 保持一致的匹配逻辑。
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
// 状态判断
// ---------------------------------------------------------------------------

/// status 为 None 或 Good 时视为 Good。
fn is_good_status(dv: &DataValue) -> bool {
    dv.status.map(|s| s.is_good()).unwrap_or(true)
}

/// 从 DataValue 提取样本时间戳: source_timestamp, 其次 server_timestamp, 否则 None。
fn sample_time(dv: &DataValue) -> Option<DateTime> {
    dv.source_timestamp.or(dv.server_timestamp)
}

// ---------------------------------------------------------------------------
// 支持的聚合函数
// ---------------------------------------------------------------------------

/// 检查聚合类型是否受支持。
pub fn aggregate_supported(agg_type: &NodeId) -> bool {
    let supported: [NodeId; 8] = [
        NodeId::from(ObjectId::AggregateFunction_Average),
        NodeId::from(ObjectId::AggregateFunction_Minimum),
        NodeId::from(ObjectId::AggregateFunction_Maximum),
        NodeId::from(ObjectId::AggregateFunction_Count),
        NodeId::from(ObjectId::AggregateFunction_TimeAverage),
        NodeId::from(ObjectId::AggregateFunction_Total),
        NodeId::from(ObjectId::AggregateFunction_Delta),
        NodeId::from(ObjectId::AggregateFunction_PercentGood),
    ];
    supported.iter().any(|id| id == agg_type)
}

/// 获取聚合类型的 ObjectId (用于内部匹配),不支持的返回 None。
fn agg_object_id(agg_type: &NodeId) -> Option<ObjectId> {
    let average: NodeId = NodeId::from(ObjectId::AggregateFunction_Average);
    let minimum: NodeId = NodeId::from(ObjectId::AggregateFunction_Minimum);
    let maximum: NodeId = NodeId::from(ObjectId::AggregateFunction_Maximum);
    let count: NodeId = NodeId::from(ObjectId::AggregateFunction_Count);
    let time_avg: NodeId = NodeId::from(ObjectId::AggregateFunction_TimeAverage);
    let total: NodeId = NodeId::from(ObjectId::AggregateFunction_Total);
    let delta: NodeId = NodeId::from(ObjectId::AggregateFunction_Delta);
    let percent_good: NodeId = NodeId::from(ObjectId::AggregateFunction_PercentGood);

    if *agg_type == average {
        Some(ObjectId::AggregateFunction_Average)
    } else if *agg_type == minimum {
        Some(ObjectId::AggregateFunction_Minimum)
    } else if *agg_type == maximum {
        Some(ObjectId::AggregateFunction_Maximum)
    } else if *agg_type == count {
        Some(ObjectId::AggregateFunction_Count)
    } else if *agg_type == time_avg {
        Some(ObjectId::AggregateFunction_TimeAverage)
    } else if *agg_type == total {
        Some(ObjectId::AggregateFunction_Total)
    } else if *agg_type == delta {
        Some(ObjectId::AggregateFunction_Delta)
    } else if *agg_type == percent_good {
        Some(ObjectId::AggregateFunction_PercentGood)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// 主聚合入口
// ---------------------------------------------------------------------------

/// 区间内按 processing_interval 切桶聚合;返回桶序列 DataValue(数值 + 桶起始时间作 source_timestamp)。
///
/// # 参数
/// - `samples`: 全量样本切片
/// - `start`: 区间起始
/// - `end`: 区间结束
/// - `processing_interval`: 桶宽度(秒, f64)
/// - `agg_type`: 聚合函数 NodeId
///
/// # 返回
/// - 每个桶一个 DataValue,source_timestamp = 桶起始时间
/// - 空桶 → Variant::Empty
/// - 不支持的聚合函数 → `Err(StatusCode::BadAggregateNotSupported)`
pub fn aggregate_samples(
    samples: &[DataValue],
    start: DateTime,
    end: DateTime,
    processing_interval: f64,
    agg_type: &NodeId,
) -> Result<Vec<DataValue>, StatusCode> {
    if processing_interval <= 0.0 {
        return Err(StatusCode::BadHistoryOperationInvalid);
    }

    let agg_id = agg_object_id(agg_type).ok_or(StatusCode::BadAggregateNotSupported)?;

    let interval = ChronoDuration::milliseconds((processing_interval * 1000.0) as i64);

    let mut results: Vec<DataValue> = Vec::new();
    let mut bucket_start = start;

    while bucket_start < end {
        let bucket_end = bucket_start + interval;

        // 收集落在 [bucket_start, bucket_end) 内的样本
        let bucket_samples: Vec<&DataValue> = samples
            .iter()
            .filter(|dv| {
                sample_time(dv)
                    .map(|t| t >= bucket_start && t < bucket_end)
                    .unwrap_or(false)
            })
            .collect();

        let value = compute_aggregate(&bucket_samples, &agg_id, bucket_end);

        let mut dv = DataValue::new_at(value, bucket_start);
        dv.status = Some(StatusCode::Good);
        results.push(dv);

        bucket_start = bucket_end;
    }

    Ok(results)
}

// ---------------------------------------------------------------------------
// 单桶聚合计算
// ---------------------------------------------------------------------------

fn compute_aggregate(bucket: &[&DataValue], agg_id: &ObjectId, bucket_end: DateTime) -> Variant {
    match *agg_id {
        ObjectId::AggregateFunction_Average => compute_average(bucket),
        ObjectId::AggregateFunction_Minimum => compute_minimum(bucket),
        ObjectId::AggregateFunction_Maximum => compute_maximum(bucket),
        ObjectId::AggregateFunction_Count => compute_count(bucket),
        ObjectId::AggregateFunction_TimeAverage => compute_time_average(bucket, bucket_end),
        ObjectId::AggregateFunction_Total => compute_total(bucket),
        ObjectId::AggregateFunction_Delta => compute_delta(bucket),
        ObjectId::AggregateFunction_PercentGood => compute_percent_good(bucket),
        _ => Variant::Empty,
    }
}

// ---------------------------------------------------------------------------
// 各聚合函数实现
// ---------------------------------------------------------------------------

/// 算术平均: 非数值样本跳过,空桶 → Empty。
fn compute_average(bucket: &[&DataValue]) -> Variant {
    let nums: Vec<f64> = bucket
        .iter()
        .filter_map(|dv| dv.value.as_ref().and_then(variant_to_f64))
        .collect();
    if nums.is_empty() {
        return Variant::Empty;
    }
    let sum: f64 = nums.iter().sum();
    Variant::Double(sum / nums.len() as f64)
}

/// 最小值。空桶 → Empty。
fn compute_minimum(bucket: &[&DataValue]) -> Variant {
    let min = bucket
        .iter()
        .filter_map(|dv| dv.value.as_ref().and_then(variant_to_f64))
        .fold(f64::NAN, |a, b| if a.is_nan() { b } else { a.min(b) });
    if min.is_nan() {
        Variant::Empty
    } else {
        Variant::Double(min)
    }
}

/// 最大值。空桶 → Empty。
fn compute_maximum(bucket: &[&DataValue]) -> Variant {
    let max = bucket
        .iter()
        .filter_map(|dv| dv.value.as_ref().and_then(variant_to_f64))
        .fold(f64::NAN, |a, b| if a.is_nan() { b } else { a.max(b) });
    if max.is_nan() {
        Variant::Empty
    } else {
        Variant::Double(max)
    }
}

/// 计数: 可转为 f64 的样本数。
fn compute_count(bucket: &[&DataValue]) -> Variant {
    let count = bucket
        .iter()
        .filter(|dv| dv.value.as_ref().and_then(variant_to_f64).is_some())
        .count();
    Variant::UInt64(count as u64)
}

/// 时间加权平均: Σ(v_i * Δt_i) / ΣΔt_i, Δt_i = 到下一个样本的时间(末样本到桶结束)。
/// 非数值样本跳过。
fn compute_time_average(bucket: &[&DataValue], bucket_end: DateTime) -> Variant {
    if bucket.is_empty() {
        return Variant::Empty;
    }

    let timed: Vec<(DateTime, f64)> = bucket
        .iter()
        .filter_map(|dv| {
            let t = sample_time(dv)?;
            let v = dv.value.as_ref().and_then(variant_to_f64)?;
            Some((t, v))
        })
        .collect();

    if timed.is_empty() {
        return Variant::Empty;
    }

    let mut total_weighted = 0.0_f64;
    let mut total_duration = 0.0_f64;

    for i in 0..timed.len() {
        let (t_i, v_i) = timed[i];
        let t_next = if i + 1 < timed.len() {
            timed[i + 1].0
        } else {
            bucket_end
        };
        let delta: ChronoDuration = t_next - t_i;
        let dt = delta.num_milliseconds() as f64 / 1000.0;
        let dt = if dt < 0.0 { 0.0 } else { dt };
        total_weighted += v_i * dt;
        total_duration += dt;
    }

    if total_duration <= 0.0 {
        Variant::Empty
    } else {
        Variant::Double(total_weighted / total_duration)
    }
}

/// 数值求和。空桶 → 0.0。
fn compute_total(bucket: &[&DataValue]) -> Variant {
    let sum: f64 = bucket
        .iter()
        .filter_map(|dv| dv.value.as_ref().and_then(variant_to_f64))
        .sum();
    Variant::Double(sum)
}

/// 桶内末值 - 首值。空桶或仅一个数值样本 → Empty。
fn compute_delta(bucket: &[&DataValue]) -> Variant {
    let nums: Vec<f64> = bucket
        .iter()
        .filter_map(|dv| dv.value.as_ref().and_then(variant_to_f64))
        .collect();
    if nums.len() < 2 {
        Variant::Empty
    } else {
        Variant::Double(nums.last().unwrap() - nums.first().unwrap())
    }
}

/// PercentGood: Good 样本数 / 桶内样本总数。
/// status 为 None 视为 Good。
fn compute_percent_good(bucket: &[&DataValue]) -> Variant {
    if bucket.is_empty() {
        return Variant::Double(0.0);
    }
    let total = bucket.len() as f64;
    let good = bucket.iter().filter(|dv| is_good_status(dv)).count() as f64;
    Variant::Double(good / total)
}

// ---------------------------------------------------------------------------
// 测试
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use opcua_types::DataValue;

    /// 辅助: 构造确定性的 DateTime (2026-01-01 + 指定秒数)
    fn dt(secs: i64) -> DateTime {
        let m = (secs / 60) as u16;
        let s = (secs % 60) as u16;
        DateTime::ymd_hms(2026, 1, 1, 0, m, s)
    }

    /// 辅助: 构造 Good 状态的 DataValue(Int64)
    fn dv_int(ts_secs: i64, value: i64) -> DataValue {
        DataValue::new_at(Variant::Int64(value), dt(ts_secs))
    }

    /// 辅助: 构造 Double 值的 DataValue
    fn dv_double(ts_secs: i64, value: f64) -> DataValue {
        DataValue::new_at(Variant::Double(value), dt(ts_secs))
    }

    /// 辅助: 构造带 Bad 状态的 DataValue
    fn dv_bad(ts_secs: i64, value: i64) -> DataValue {
        let mut dv = DataValue::new_at(Variant::Int64(value), dt(ts_secs));
        dv.status = Some(StatusCode::Bad);
        dv
    }

    /// 辅助: 构造 String 值样本(不可转 f64)
    fn dv_str(ts_secs: i64, s: &str) -> DataValue {
        DataValue::new_at(Variant::String(s.into()), dt(ts_secs))
    }

    /// 辅助: 构造无 status 的 DataValue
    fn dv_no_status(ts_secs: i64, value: i64) -> DataValue {
        DataValue::new_at(Variant::Int64(value), dt(ts_secs))
    }

    /// 辅助: 聚合函数 NodeId
    fn agg_id(id: ObjectId) -> NodeId {
        id.into()
    }

    // =======================================================================
    // aggregate_supported
    // =======================================================================

    #[test]
    fn supported_returns_true_for_known_aggregates() {
        assert!(aggregate_supported(&agg_id(
            ObjectId::AggregateFunction_Average
        )));
        assert!(aggregate_supported(&agg_id(
            ObjectId::AggregateFunction_Minimum
        )));
        assert!(aggregate_supported(&agg_id(
            ObjectId::AggregateFunction_Maximum
        )));
        assert!(aggregate_supported(&agg_id(
            ObjectId::AggregateFunction_Count
        )));
        assert!(aggregate_supported(&agg_id(
            ObjectId::AggregateFunction_TimeAverage
        )));
        assert!(aggregate_supported(&agg_id(
            ObjectId::AggregateFunction_Total
        )));
        assert!(aggregate_supported(&agg_id(
            ObjectId::AggregateFunction_Delta
        )));
        assert!(aggregate_supported(&agg_id(
            ObjectId::AggregateFunction_PercentGood
        )));
    }

    #[test]
    fn supported_returns_false_for_unknown_aggregate() {
        assert!(!aggregate_supported(&agg_id(
            ObjectId::AggregateFunction_AnnotationCount
        )));
        assert!(!aggregate_supported(&agg_id(
            ObjectId::AggregateFunction_Range
        )));
    }

    // =======================================================================
    // Average
    // =======================================================================

    #[test]
    fn average_basic() {
        let samples = vec![dv_int(10, 10), dv_int(20, 20), dv_int(30, 30)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Average),
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value.as_ref().unwrap().to_string(), "20");
    }

    #[test]
    fn average_empty_bucket() {
        let result = aggregate_samples(
            &[],
            dt(0),
            dt(120),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Average),
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0].value.as_ref().unwrap(), Variant::Empty));
        assert!(matches!(result[1].value.as_ref().unwrap(), Variant::Empty));
    }

    #[test]
    fn average_skips_non_numeric() {
        let samples = vec![dv_int(10, 10), dv_str(15, "hello"), dv_int(20, 20)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Average),
        )
        .unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].value.as_ref().unwrap().to_string(), "15");
    }

    // =======================================================================
    // Minimum / Maximum
    // =======================================================================

    #[test]
    fn minimum_basic() {
        let samples = vec![dv_int(10, 5), dv_int(20, 3), dv_int(30, 7)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Minimum),
        )
        .unwrap();
        assert_eq!(result[0].value.as_ref().unwrap().to_string(), "3");
    }

    #[test]
    fn maximum_basic() {
        let samples = vec![dv_int(10, 5), dv_int(20, 3), dv_int(30, 7)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Maximum),
        )
        .unwrap();
        assert_eq!(result[0].value.as_ref().unwrap().to_string(), "7");
    }

    #[test]
    fn minimum_empty_bucket() {
        let result = aggregate_samples(
            &[],
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Minimum),
        )
        .unwrap();
        assert!(matches!(result[0].value.as_ref().unwrap(), Variant::Empty));
    }

    // =======================================================================
    // Count
    // =======================================================================

    #[test]
    fn count_only_numeric() {
        let samples = vec![dv_int(10, 1), dv_str(15, "x"), dv_int(20, 2), dv_int(30, 3)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Count),
        )
        .unwrap();
        assert_eq!(result[0].value.as_ref().unwrap().to_string(), "3");
    }

    #[test]
    fn count_empty() {
        let result = aggregate_samples(
            &[],
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Count),
        )
        .unwrap();
        let val = result[0].value.as_ref().unwrap();
        assert!(matches!(val, Variant::UInt64(0)), "expected 0, got {val:?}");
    }

    // =======================================================================
    // TimeAverage
    // =======================================================================

    #[test]
    fn time_average_weighted() {
        // ts=10,v=10; ts=16,v=16; bucket_end=30
        // Δt₁=6, Δt₂=14; weighted=10*6+16*14=284; total=20; avg=14.2
        let samples = vec![dv_double(10, 10.0), dv_double(16, 16.0)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(30),
            30.0,
            &agg_id(ObjectId::AggregateFunction_TimeAverage),
        )
        .unwrap();
        let val = result[0].value.as_ref().unwrap();
        assert!(
            matches!(val, Variant::Double(x) if (x - 14.2).abs() < 0.001),
            "expected 14.2, got {val:?}"
        );
    }

    #[test]
    fn time_average_single_sample() {
        let samples = vec![dv_double(10, 10.0)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_TimeAverage),
        )
        .unwrap();
        let val = result[0].value.as_ref().unwrap();
        assert!(
            matches!(val, Variant::Double(x) if (x - 10.0).abs() < 0.001),
            "expected 10.0, got {val:?}"
        );
    }

    #[test]
    fn time_average_empty() {
        let result = aggregate_samples(
            &[],
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_TimeAverage),
        )
        .unwrap();
        assert!(matches!(result[0].value.as_ref().unwrap(), Variant::Empty));
    }

    // =======================================================================
    // Total
    // =======================================================================

    #[test]
    fn total_sum() {
        let samples = vec![dv_int(10, 10), dv_int(20, 20), dv_int(30, 30)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Total),
        )
        .unwrap();
        assert_eq!(result[0].value.as_ref().unwrap().to_string(), "60");
    }

    #[test]
    fn total_empty() {
        let result = aggregate_samples(
            &[],
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Total),
        )
        .unwrap();
        let val = result[0].value.as_ref().unwrap();
        assert!(
            matches!(val, Variant::Double(x) if *x == 0.0),
            "expected 0.0, got {val:?}"
        );
    }

    // =======================================================================
    // Delta
    // =======================================================================

    #[test]
    fn delta_basic() {
        let samples = vec![dv_int(10, 10), dv_int(20, 20), dv_int(30, 30)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Delta),
        )
        .unwrap();
        assert_eq!(result[0].value.as_ref().unwrap().to_string(), "20");
    }

    #[test]
    fn delta_single_sample() {
        let samples = vec![dv_int(10, 10)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Delta),
        )
        .unwrap();
        assert!(matches!(result[0].value.as_ref().unwrap(), Variant::Empty));
    }

    #[test]
    fn delta_empty() {
        let result = aggregate_samples(
            &[],
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Delta),
        )
        .unwrap();
        assert!(matches!(result[0].value.as_ref().unwrap(), Variant::Empty));
    }

    // =======================================================================
    // PercentGood
    // =======================================================================

    #[test]
    fn percent_good_basic() {
        let samples = vec![dv_int(10, 1), dv_bad(20, 2), dv_int(30, 3), dv_int(40, 4)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_PercentGood),
        )
        .unwrap();
        let val = result[0].value.as_ref().unwrap();
        assert!(
            matches!(val, Variant::Double(x) if (x - 0.75).abs() < 0.001),
            "expected 0.75, got {val:?}"
        );
    }

    #[test]
    fn percent_good_none_status_is_good() {
        let samples = vec![dv_no_status(10, 1), dv_bad(20, 2)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_PercentGood),
        )
        .unwrap();
        let val = result[0].value.as_ref().unwrap();
        assert!(
            matches!(val, Variant::Double(x) if (x - 0.5).abs() < 0.001),
            "expected 0.5, got {val:?}"
        );
    }

    #[test]
    fn percent_good_empty() {
        let result = aggregate_samples(
            &[],
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_PercentGood),
        )
        .unwrap();
        let val = result[0].value.as_ref().unwrap();
        assert!(
            matches!(val, Variant::Double(x) if *x == 0.0),
            "expected 0.0, got {val:?}"
        );
    }

    // =======================================================================
    // 桶分切边界
    // =======================================================================

    #[test]
    fn bucket_boundary_sample_at_boundary_goes_to_next_bucket() {
        let samples = vec![dv_int(0, 10), dv_int(60, 20)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(120),
            60.0,
            &agg_id(ObjectId::AggregateFunction_Average),
        )
        .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].value.as_ref().unwrap().to_string(), "10");
        assert_eq!(result[1].value.as_ref().unwrap().to_string(), "20");
    }

    #[test]
    fn multi_bucket_distribution() {
        let samples = vec![dv_int(10, 10), dv_int(40, 40), dv_int(70, 70)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(90),
            30.0,
            &agg_id(ObjectId::AggregateFunction_Average),
        )
        .unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].value.as_ref().unwrap().to_string(), "10");
        assert_eq!(result[1].value.as_ref().unwrap().to_string(), "40");
        assert_eq!(result[2].value.as_ref().unwrap().to_string(), "70");
        assert_eq!(result[0].source_timestamp.unwrap(), dt(0));
        assert_eq!(result[1].source_timestamp.unwrap(), dt(30));
        assert_eq!(result[2].source_timestamp.unwrap(), dt(60));
    }

    // =======================================================================
    // 不支持的聚合函数
    // =======================================================================

    #[test]
    fn unsupported_aggregate_returns_error() {
        let samples = vec![dv_int(10, 10)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            60.0,
            &agg_id(ObjectId::AggregateFunction_AnnotationCount),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BadAggregateNotSupported);
    }

    // =======================================================================
    // processing_interval 校验
    // =======================================================================

    #[test]
    fn non_positive_processing_interval_returns_error() {
        let samples = vec![dv_int(10, 10)];
        let result = aggregate_samples(
            &samples,
            dt(0),
            dt(60),
            0.0,
            &agg_id(ObjectId::AggregateFunction_Average),
        );
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), StatusCode::BadHistoryOperationInvalid);
    }
}
