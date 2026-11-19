//! Compare the winner for two Nets with a direct search in the specification's order.

use proptest::prelude::*;
use reda_model::{RuleField, RuleKind, Uuid};
use reda_pcb::rules::resolve_pair_on;
use reda_testkit::{TempDir, probe_engine};
use serde_json::{Value, json};

fn id(result: Value) -> Uuid {
    serde_json::from_value(result["result"]["uuid"].clone()).unwrap()
}

// Owner indices 0 and 1 are ranked classes A and B, index 2 is Default.
const PAIRS: [(usize, usize); 6] = [(0, 0), (0, 1), (0, 2), (1, 1), (1, 2), (2, 2)];
const CLASSES: [&str; 3] = ["A", "B", "Default"];

proptest! {
    #![proptest_config(ProptestConfig { cases: 24, ..ProptestConfig::default() })]
    #[test]
    fn value_on_a_plane_matches_explicit_pair_then_class_then_board_search(
        membership in 0u8..16, rows in prop::collection::vec((0usize..9,0usize..4,1i64..500,any::<bool>()),1..25)
    ) {
        let dir=TempDir::new("reference-rule-order");
        let mut e=probe_engine();
        e.run_ok("project.new",json!({"path":dir.path(),"name":"Order","layers":4}));
        let layer=e.project().unwrap().pcb().unwrap().copper_layers().nth(1).unwrap().uuid;
        e.run_ok("pcb.layer.set",json!({"layer":layer,"role":"plane"}));
        let nets=["P","N"].map(|name|id(e.run_ok("test.net.add",json!({"name":name}))));
        let has=|net:usize,class:usize| class==2 || membership & (1 << (net*2+class)) != 0;
        let classes=[0,1].map(|c| {
            let members:Vec<_>=(0..2).filter(|&n|has(n,c)).map(|n|nets[n]).collect();
            id(e.run_ok("pcb.net_class.add",json!({"name":CLASSES[c],"nets":members})))
        });
        // Sorted by scope order up front, independent of storage order and the production comparator.
        let mut table=[[None;4];9];
        let board=e.project().unwrap().pcb().unwrap().board_rule(RuleKind::Clearance).uuid;
        table[8][3]=Some((160_000,board));
        for (owner,specificity,value,enabled) in rows {
            let mut scope=json!({});
            if let Some(&(a,b))=PAIRS.get(owner) {scope["net_class_pair"]=json!([CLASSES[b],CLASSES[a]]);}
            else if owner<8 {scope["net_class"]=json!(classes[owner-6]);}
            if specificity !=3 {scope["layers"]=[json!([layer]),json!("plane"),json!("inner")][specificity].clone();}
            let r=e.run_ok("pcb.rules.set",json!({"kind":"clearance","scope":scope,"values":{"value":value*1000},"enabled":enabled}));
            let rule=serde_json::from_value(r["result"]["rule"]["uuid"].clone()).unwrap();
            table[owner][specificity]=enabled.then_some((value*1000,rule));
        }
        let first=|owner:usize|table[owner].iter().copied().flatten().next();
        let pair=PAIRS.iter().enumerate().filter(|(_, (a,b))|
                has(0,*a)&&has(1,*b)||has(1,*a)&&has(0,*b)).find_map(|(i,_)|first(i));
        let own=|n| (0..3).filter(|&c|has(n,c)).find_map(|c|first(6+c));
        let expected=pair.or_else(||match (own(0),own(1)) {
            (Some(a),Some(b)) if a.0>=b.0 => Some(a),
            (Some(_),b @ Some(_))=>b,
            (a,b)=>a.or(b),
        });
        let found=resolve_pair_on(e.project().unwrap().pcb().unwrap(),Some(nets[0]),Some(nets[1]),Some(layer),RuleKind::Clearance,RuleField::Value)
            .map(|s|(s.value.length().unwrap().0,s.rule));
        prop_assert_eq!(found,expected);
    }
}
