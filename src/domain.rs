#![allow(dead_code)]

use crate::date::Weekday;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RestaurantId {
    JinxEmpire,
    Vallagata,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestaurantMeta {
    pub id: RestaurantId,
    pub display_name: &'static str,
    pub source_url: &'static str,
    pub source_kind: SourceKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceKind {
    JsonLdMenu,
    HtmlWeekdayText,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestaurantLunch {
    pub meta: RestaurantMeta,
    pub state: LunchState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LunchState {
    Available {
        weekday: Weekday,
        items: Vec<LunchItem>,
        notes: Vec<String>,
    },
    NoLunchToday {
        weekday: Weekday,
        reason: NoLunchReason,
    },
    Unavailable {
        stage: FailureStage,
        error: SourceError,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LunchItem {
    pub title: Option<String>,
    pub description: String,
    pub kind: MenuItemKind,
    pub price: Option<Price>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItemKind {
    Meat,
    Vegetarian,
    Vegan,
    Fish,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Price {
    pub amount: u32,
    pub currency: Currency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Currency {
    Sek,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoLunchReason {
    Weekend,
    Closed,
    MissingDay,
    EmptyMenu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureStage {
    Fetch,
    Parse,
    Normalize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceError {
    NotImplemented,
    Network(String),
    HttpStatus(u16),
    MissingStructuredData,
    MissingExpectedElement(&'static str),
    InvalidJson(String),
    InvalidPrice(String),
    UnsupportedFormat(String),
}
