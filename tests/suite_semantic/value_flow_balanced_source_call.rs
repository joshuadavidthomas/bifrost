//! Issue #1951: the balanced source-call scenario on the direct
//! `ValueFlowPlan` route.
//!
//! Each language runs the exact DataFlowBench shape: the positive routes the
//! source call's returned value into sink argument 0 and must produce one
//! proven meeting with a complete witness; the negative leaves the source
//! result unused, passes an independent constant to the sink, and must stay a
//! meaningful negative (clean when the language completes, typed-incomplete
//! otherwise).

use crate::value_flow_conformance::assert_value_flow_conformance;
use crate::value_flow_scenarios::{
    assert_balanced_source_call_scenario_inventory, balanced_source_call_scenario_entries,
    c_balanced_source_call_shape, cpp_balanced_source_call_shape,
    csharp_balanced_source_call_shape, go_balanced_source_call_shape,
    java_balanced_source_call_shape, javascript_balanced_source_call_shape,
    kotlin_balanced_source_call_shape, php_balanced_source_call_shape,
    python_balanced_source_call_shape, ruby_balanced_source_call_shape,
    rust_balanced_source_call_shape, scala_balanced_source_call_shape,
    typescript_balanced_source_call_shape, with_balanced_source_call_negative,
    with_balanced_source_call_positive,
};

macro_rules! define_direct_balanced_source_call_tests {
    ($(($name:ident, $shape:ident, $direct_positive:ident, $direct_negative:ident, $public_positive:ident, $public_negative:ident),)*) => {
        $(
            #[test]
            fn $direct_positive() {
                with_balanced_source_call_positive(&$shape(), assert_value_flow_conformance);
            }

            #[test]
            fn $direct_negative() {
                with_balanced_source_call_negative(&$shape(), assert_value_flow_conformance);
            }
        )*
    };
}
balanced_source_call_scenario_entries!(define_direct_balanced_source_call_tests);

#[test]
fn balanced_source_call_scenario_inventory_is_complete() {
    assert_balanced_source_call_scenario_inventory();
}
