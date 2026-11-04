//! The design rules that apply to one Net: its Net Class's overrides over the board-wide
//! rules.

use reda_model::geom::Nm;
use reda_model::{PcbDocument, Uuid};

/// The rules for copper on one Net (or on no Net, which takes the Default Net Class).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NetRules {
    pub clearance: Nm,
    pub track_width: Nm,
    pub via_diameter: Nm,
    pub via_hole: Nm,
}

/// The rules for copper on `net`: its Net Class's overrides, else the board-wide rules.
/// Copper on no Net (or an unknown one) uses the Default Net Class.
pub fn net_rules(pcb: &PcbDocument, net: Option<Uuid>) -> NetRules {
    let setup = pcb.setup();
    let class = net
        .and_then(|n| pcb.net(n))
        .map_or(setup.default_net_class, |n| n.class);
    let class = pcb.net_class(class);
    let rules = setup.rules;
    NetRules {
        clearance: class.and_then(|c| c.clearance).unwrap_or(rules.clearance),
        track_width: class
            .and_then(|c| c.track_width)
            .unwrap_or(rules.track_width),
        via_diameter: class
            .and_then(|c| c.via_diameter)
            .unwrap_or(rules.via_diameter),
        via_hole: class.and_then(|c| c.via_hole).unwrap_or(rules.via_hole),
    }
}

/// The clearance required between copper on Nets `a` and `b`: the larger of their Net
/// Classes' clearances.
pub fn clearance_between(pcb: &PcbDocument, a: Option<Uuid>, b: Option<Uuid>) -> Nm {
    net_rules(pcb, a).clearance.max(net_rules(pcb, b).clearance)
}
