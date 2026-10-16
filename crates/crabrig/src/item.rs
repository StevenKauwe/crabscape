//! What the puppet builders return: draw-ordered shapes, lines and glows in crab space.

use crate::geom::{M, P, ap};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Shape,
    Line,
    Glow,
    /// A flat background wash (the model sheet only).
    Wash,
    /// A soft watercolour background shape (the model sheet only).
    Fill,
}

/// One drawn thing. `ink: false` = no outline (`ink: null` in the JS). Glows use `x, y, r, col, a`.
#[derive(Clone, Debug)]
pub struct Item {
    pub k: Kind,
    pub id: String,
    pub g: Option<String>,
    pub pts: Vec<P>,
    pub mat: String,
    pub sw: Option<f64>,
    pub ink: bool,
    pub op: Option<f64>,
    pub far: bool,
    pub mid: bool,
    pub shade: bool,
    pub under: bool,
    /// The joint pivot (crab space; not moved by `transform_items`).
    pub pv: Option<P>,
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub col: String,
    pub a: Option<f64>,
    /// Fill/wash only: bleed.
    pub bleed: Option<f64>,
}

impl Item {
    pub fn new(k: Kind, id: impl Into<String>, g: Option<&str>, pts: Vec<P>, mat: &str) -> Item {
        Item {
            k,
            id: id.into(),
            g: g.map(str::to_string),
            pts,
            mat: mat.to_string(),
            sw: None,
            ink: true,
            op: None,
            far: false,
            mid: false,
            shade: false,
            under: false,
            pv: None,
            x: 0.0,
            y: 0.0,
            r: 0.0,
            col: String::new(),
            a: None,
            bleed: None,
        }
    }
    pub fn glow(
        id: impl Into<String>,
        g: Option<&str>,
        x: f64,
        y: f64,
        r: f64,
        col: &str,
        a: Option<f64>,
    ) -> Item {
        let mut it = Item::new(Kind::Glow, id, g, vec![], "");
        (it.x, it.y, it.r, it.col, it.a) = (x, y, r, col.to_string(), a);
        it
    }
    pub fn sw(mut self, v: f64) -> Self {
        self.sw = Some(v);
        self
    }
    pub fn no_ink(mut self) -> Self {
        self.ink = false;
        self
    }
    pub fn op(mut self, v: f64) -> Self {
        self.op = Some(v);
        self
    }
    pub fn pv(mut self, v: P) -> Self {
        self.pv = Some(v);
        self
    }
    pub fn shade(mut self) -> Self {
        self.shade = true;
        self
    }
    pub fn far(mut self) -> Self {
        self.far = true;
        self
    }
}

/// Move items through a transform (glows: centre moved, radius scaled). Pivots stay as they are.
pub fn transform_items(items: &[Item], m: M) -> Vec<Item> {
    items
        .iter()
        .map(|it| {
            let mut o = it.clone();
            if it.k == Kind::Glow {
                let p = ap(m, [it.x, it.y]);
                (o.x, o.y, o.r) = (p[0], p[1], it.r * m[0].hypot(m[1]));
            } else {
                o.pts = it.pts.iter().map(|p| ap(m, *p)).collect();
            }
            o
        })
        .collect()
}
