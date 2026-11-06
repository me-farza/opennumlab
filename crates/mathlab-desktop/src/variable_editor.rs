//! Paged array editor. Reads and writes are sent to the engine worker; rendering never executes code.

use crate::{variable_bytes, variable_size, BLUE, MUTED, PALE, RED};
use eframe::egui::{self, RichText};
use mathlab_engine::{
    BindingToken, BoundVariableSlice, NumericClass, NumericComponents, PageData, ValuePage,
    Variable, VariableSlice,
};
use std::collections::BTreeMap;

pub const PAGE_ROWS: usize = 30;
pub const PAGE_COLUMNS: usize = 12;

pub struct VariableEditor {
    pub native_readonly:bool,
    native_note:Option<String>,
    pub variable: Variable,
    pub row_start: usize,
    pub column_start: usize,
    pub higher_indices: Vec<u64>,
    pub request_id: u64,
    pub pending: bool,
    pub stale: bool,
    pub error: Option<String>,
    pub slice: Option<VariableSlice>,
    pub token: Option<BindingToken>,
    pub typed_page: Option<ValuePage>,
    /// Transaction uncertainty blocks mutation without disabling pure reads.
    pub writes_blocked: bool,
    pending_read: Option<PendingRead>,
    selected: Option<(usize, usize)>,
    literal: String,
}

pub enum Action {
    Read,
    Refresh,
    Write {
        token: BindingToken,
        row: usize,
        column: usize,
        literal: String,
    },
    Display(String),
}

pub enum VariablePage {
    Typed {
        page: Box<ValuePage>,
        /// Present only when this read explicitly acquired a fresh snapshot.
        metadata: Option<Variable>,
    },
    Checked(BoundVariableSlice),
    Preview(VariableSlice),
    /// A fresh unconditional snapshot can correct a stale selection draft even
    /// when its page read failed. This outcome never authorizes editing.
    MetadataOnly {
        variable: Variable,
        diagnostic: String,
    },
}

struct PendingRead {
    token: Option<BindingToken>,
    row_start: usize,
    column_start: usize,
    higher_indices: Vec<u64>,
}

fn float_text(value: f64) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value.is_infinite() {
        if value.is_sign_negative() {
            "-Inf".into()
        } else {
            "Inf".into()
        }
    } else if value == 0.0 && value.is_sign_negative() {
        "-0".into()
    } else {
        value.to_string()
    }
}

fn single_text(value: f32) -> String {
    if value.is_nan() {
        "NaN".into()
    } else if value.is_infinite() {
        if value.is_sign_negative() {
            "-Inf".into()
        } else {
            "Inf".into()
        }
    } else if value == 0.0 && value.is_sign_negative() {
        "-0".into()
    } else {
        value.to_string()
    }
}

fn component_text(values: &NumericComponents, index: usize) -> Result<String, String> {
    let text = match values {
        NumericComponents::Double(values) => values.get(index).map(|&value| float_text(value)),
        NumericComponents::Single(values) => values.get(index).map(|&value| single_text(value)),
        NumericComponents::Int8(values) => values.get(index).map(ToString::to_string),
        NumericComponents::Uint8(values) => values.get(index).map(ToString::to_string),
        NumericComponents::Int16(values) => values.get(index).map(ToString::to_string),
        NumericComponents::Uint16(values) => values.get(index).map(ToString::to_string),
        NumericComponents::Int32(values) => values.get(index).map(ToString::to_string),
        NumericComponents::Uint32(values) => values.get(index).map(ToString::to_string),
        NumericComponents::Int64(values) => values.get(index).map(ToString::to_string),
        NumericComponents::Uint64(values) => values.get(index).map(ToString::to_string),
        NumericComponents::Logical(values) => values.get(index).map(ToString::to_string),
    };
    text.ok_or_else(|| "The typed page has an invalid component count.".into())
}

fn scalar_text(
    real: &NumericComponents,
    imag: Option<&NumericComponents>,
    index: usize,
) -> Result<String, String> {
    let real = component_text(real, index)?;
    match imag {
        None => Ok(real),
        Some(imag) => {
            let imag = component_text(imag, index)?;
            let sign = if imag.starts_with('-') { "" } else { "+" };
            Ok(format!("{real}{sign}{imag}i"))
        }
    }
}

fn class_name(class: NumericClass) -> &'static str {
    match class {
        NumericClass::Double => "double",
        NumericClass::Single => "single",
        NumericClass::Int8 => "int8",
        NumericClass::Uint8 => "uint8",
        NumericClass::Int16 => "int16",
        NumericClass::Uint16 => "uint16",
        NumericClass::Int32 => "int32",
        NumericClass::Uint32 => "uint32",
        NumericClass::Int64 => "int64",
        NumericClass::Uint64 => "uint64",
        NumericClass::Logical => "logical",
    }
}

/// Build only the bounded grid's display strings. The canonical typed payload
/// remains in the editor, including sparse COO coordinates and exact integers.
fn typed_projection(page: &ValuePage) -> Result<VariableSlice, String> {
    let rows = usize::try_from(page.selection.rows)
        .map_err(|_| "Typed page rows cannot be represented on this platform.")?;
    let columns = usize::try_from(page.selection.columns)
        .map_err(|_| "Typed page columns cannot be represented on this platform.")?;
    if rows > 2_000 || columns > 2_000 {
        return Err("Typed page exceeds the bounded display extent.".into());
    }
    let count = rows
        .checked_mul(columns)
        .filter(|&count| count <= 2_000)
        .ok_or("Typed page exceeds the bounded display cell count.")?;
    let dimensions = page
        .source_shape
        .iter()
        .map(|extent| extent.checked_usize().map_err(|error| error.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    let (real, imag, sparse) = match &page.data {
        PageData::Dense { real, imag } => {
            if real.len() != count || imag.as_ref().is_some_and(|values| values.len() != count) {
                return Err("The typed page has an invalid dense component count.".into());
            }
            (real, imag.as_ref(), None)
        }
        PageData::SparseCoo {
            rows: row_indices,
            columns: column_indices,
            real,
            imag,
        } => {
            if row_indices.len() > count
                || row_indices.len() != column_indices.len()
                || real.len() != row_indices.len()
                || imag
                    .as_ref()
                    .is_some_and(|values| values.len() != real.len())
            {
                return Err("The typed page has invalid sparse component counts.".into());
            }
            let mut lookup = BTreeMap::new();
            for (index, (&row, &column)) in row_indices.iter().zip(column_indices).enumerate() {
                let row = usize::try_from(row).map_err(|_| "Invalid sparse row.")?;
                let column = usize::try_from(column).map_err(|_| "Invalid sparse column.")?;
                if row == 0
                    || column == 0
                    || row > rows
                    || column > columns
                    || lookup.insert((row - 1, column - 1), index).is_some()
                {
                    return Err("The typed page has invalid sparse coordinates.".into());
                }
            }
            (real, imag.as_ref(), Some(lookup))
        }
    };
    if page.complex != imag.is_some() {
        return Err("The typed page has inconsistent complex components.".into());
    }
    let zero = if page.complex {
        "0+0i"
    } else if page.class == NumericClass::Logical {
        "false"
    } else {
        "0"
    };
    let mut cells = Vec::with_capacity(rows);
    for row in 0..rows {
        let mut line = Vec::with_capacity(columns);
        for column in 0..columns {
            let index = match &sparse {
                Some(lookup) => lookup.get(&(row, column)).copied(),
                None => Some(row + column * rows),
            };
            line.push(match index {
                Some(index) => scalar_text(real, imag, index)?,
                None => zero.into(),
            });
        }
        cells.push(line);
    }
    Ok(VariableSlice {
        name: page.token.name.clone(),
        class_name: class_name(page.class).into(),
        dimensions,
        row_start: page
            .selection
            .row_start
            .checked_usize()
            .map_err(|error| error.to_string())?,
        column_start: page
            .selection
            .column_start
            .checked_usize()
            .map_err(|error| error.to_string())?,
        rows,
        columns,
        cells,
        // CP02 scalar writes address a root 2-D array only. In particular, a
        // selected N-D plane must never become root(row,column) by accident.
        editable: page.path.is_empty() && page.source_shape.len() == 2,
    })
}

impl VariableEditor {
    pub fn new(variable: Variable) -> Self {
        let higher_indices = vec![1; variable.dimensions.len().saturating_sub(2)];
        Self {
            native_readonly:false,native_note:None,
            variable,
            row_start: 1,
            column_start: 1,
            higher_indices,
            request_id: 0,
            pending: false,
            stale: true,
            error: None,
            slice: None,
            token: None,
            typed_page: None,
            writes_blocked: false,
            pending_read: None,
            selected: None,
            literal: String::new(),
        }
    }

    pub fn begin(&mut self, request_id: u64) {
        self.request_id = request_id;
        self.pending = true;
        self.stale = true;
        self.token = None;
        self.pending_read = None;
        self.error = None;
    }

    pub fn begin_read(&mut self, request_id: u64, token: Option<BindingToken>) {
        self.begin(request_id);
        self.pending_read = Some(PendingRead {
            token,
            row_start: self.row_start,
            column_start: self.column_start,
            higher_indices: self.higher_indices.clone(),
        });
    }

    /// Older worker replies cannot replace a newer page or variable selection.
    pub fn accept(&mut self, request_id: u64, result: Result<VariablePage, String>) {
        if self.request_id != request_id {
            return;
        }
        self.pending = false;
        self.stale = true;
        self.token = None;
        let expected = self.pending_read.take();
        match result {
            Ok(page) => {
                let (mut slice, token, typed) = match page {
                    VariablePage::MetadataOnly {
                        variable,
                        diagnostic,
                    } => {
                        if variable.name != self.variable.name
                            || expected
                                .as_ref()
                                .is_none_or(|expected| expected.token.is_some())
                        {
                            self.error = Some(
                                "The fresh metadata does not match this unconditional variable read."
                                    .into(),
                            );
                            return;
                        }
                        // Correct only the next explicit read's draft. Do not
                        // retain values or a token from the failed selection,
                        // retry it, or pretend that a page was accepted.
                        self.invalidate(Some(variable));
                        self.slice = None;
                        self.typed_page = None;
                        self.error = Some(diagnostic);
                        return;
                    }
                    VariablePage::Typed { page, metadata } => {
                        let page = *page;
                        let matched = expected.as_ref().is_some_and(|expected| {
                            page.path.is_empty()
                                && page.token.name == self.variable.name
                                && expected
                                    .token
                                    .as_ref()
                                    .is_none_or(|token| token == &page.token)
                                && u64::try_from(expected.row_start).ok()
                                    == Some(page.selection.row_start.get())
                                && u64::try_from(expected.column_start).ok()
                                    == Some(page.selection.column_start.get())
                                && expected.higher_indices.iter().copied().eq(page
                                    .selection
                                    .higher_indices
                                    .iter()
                                    .map(|index| index.get()))
                        });
                        if !matched {
                            self.error = Some("The typed page does not match the requested binding or plane. Refresh before inspecting it.".into());
                            return;
                        }
                        let slice = match typed_projection(&page) {
                            Ok(slice) => slice,
                            Err(error) => {
                                self.error = Some(error);
                                return;
                            }
                        };
                        if let Some(metadata) = metadata {
                            if metadata.name != page.token.name
                                || metadata.class_name != slice.class_name
                                || (metadata.dimensions_known
                                    && metadata.dimensions != slice.dimensions)
                            {
                                self.error = Some(
                                    "The page metadata does not match its source binding.".into(),
                                );
                                return;
                            }
                            // Bytes come only from the metadata authority, not
                            // from the displayed window or decoded components.
                            self.variable = metadata;
                        }
                        (slice, Some(page.token.clone()), Some(page))
                    }
                    VariablePage::Checked(page) => {
                        if expected.as_ref().is_some_and(|expected| {
                            expected
                                .token
                                .as_ref()
                                .is_some_and(|token| token != &page.token)
                                || expected.higher_indices.iter().any(|&index| index != 1)
                        }) {
                            self.error = Some(
                                "The checked page does not match the requested binding or plane."
                                    .into(),
                            );
                            return;
                        }
                        (page.slice, Some(page.token), None)
                    }
                    VariablePage::Preview(slice) => {
                        if expected
                            .as_ref()
                            .is_some_and(|expected| expected.token.is_some())
                        {
                            self.error = Some(
                                "A checked page cannot be replaced with an unconditional preview."
                                    .into(),
                            );
                            return;
                        }
                        (slice, None, None)
                    }
                };
                if slice.name != self.variable.name
                    || token.as_ref().is_some_and(|token| token.name != slice.name)
                {
                    self.error =
                        Some("The variable selection changed. Refresh before editing.".into());
                    return;
                }
                if token.is_none() {
                    slice.editable = false;
                }
                self.row_start = slice.row_start;
                self.column_start = slice.column_start;
                self.variable.dimensions = slice.dimensions.clone();
                self.variable.dimensions_known = true;
                self.variable.class_name = slice.class_name.clone();
                self.higher_indices = typed.as_ref().map_or_else(
                    || vec![1; slice.dimensions.len().saturating_sub(2)],
                    |page| {
                        page.selection
                            .higher_indices
                            .iter()
                            .map(|index| index.get())
                            .collect()
                    },
                );
                self.slice = Some(slice);
                self.token = token;
                self.typed_page = typed;
                self.stale = false;
                self.error = None;
                self.selected = None;
                self.literal.clear();
            }
            Err(error) => self.error = Some(error),
        }
    }

    pub fn invalidate(&mut self, variable: Option<Variable>) {
        self.stale = true;
        self.pending = false;
        self.token = None;
        self.pending_read = None;
        self.selected = None;
        self.literal.clear();
        if let Some(variable) = variable {
            self.variable = variable;
            self.row_start = self.row_start.min(self.row_count().max(1));
            self.column_start = self.column_start.min(self.column_count().max(1));
            self.higher_indices = self
                .variable
                .dimensions
                .iter()
                .skip(2)
                .enumerate()
                .map(|(axis, &extent)| {
                    self.higher_indices
                        .get(axis)
                        .copied()
                        .unwrap_or(1)
                        .min(extent.max(1) as u64)
                })
                .collect();
            self.error = None;
        } else {
            self.slice = None;
            self.typed_page = None;
            self.error = Some("This variable is no longer in the workspace.".into());
        }
    }

    fn row_count(&self) -> usize {
        self.variable.dimensions.first().copied().unwrap_or(0)
    }

    fn column_count(&self) -> usize {
        self.variable.dimensions.get(1).copied().unwrap_or(1)
    }

    fn can_edit(&self) -> bool {
        !self.native_readonly && !self.writes_blocked
            && !self.pending
            && !self.stale
            && self.token.is_some()
            && self
                .typed_page
                .as_ref()
                .is_none_or(|page| page.path.is_empty() && page.source_shape.len() == 2)
            && self
                .slice
                .as_ref()
                .is_some_and(|slice| slice.editable && slice.dimensions.len() == 2)
    }

    fn write_action(&self, row: usize, column: usize) -> Option<Action> {
        if !self.can_edit() {
            return None;
        }
        self.token.as_ref().map(|token| Action::Write {
            token: token.clone(),
            row,
            column,
            literal: self.literal.clone(),
        })
    }

    pub fn show(&mut self, ctx: &egui::Context, ready: bool) -> (bool, Option<Action>) {
        let mut open = true;
        let mut action = None;
        egui::Window::new(format!("Variable Editor — {}", self.variable.name))
            .id(egui::Id::new("variable_editor"))
            .open(&mut open)
            .default_size([990.0, 630.0])
            .min_size([650.0, 370.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&self.variable.name).monospace().size(18.0).strong().color(BLUE));
                    ui.label(RichText::new(format!("{} · {} · {}", variable_size(&self.variable), self.variable.class_name, variable_bytes(&self.variable))).color(MUTED));
                    if self.pending { ui.spinner(); }
                });
                let enabled = ready && !self.pending;
                ui.horizontal_wrapped(|ui| {
                    ui.label("Row");
                    ui.add(egui::DragValue::new(&mut self.row_start).range(1..=self.variable.dimensions.first().copied().unwrap_or(1).max(1)).speed(1));
                    ui.label("Column");
                    ui.add(egui::DragValue::new(&mut self.column_start).range(1..=self.variable.dimensions.get(1).copied().unwrap_or(1).max(1)).speed(1));
                    if ui.add_enabled(enabled, egui::Button::new("Go / Refresh")).clicked() { action = Some(Action::Refresh); }
                    ui.separator();
                    for (label, row, column, valid) in [
                        ("Prev Rows", self.row_start.saturating_sub(PAGE_ROWS).max(1), self.column_start, self.row_start > 1),
                        ("Next Rows", self.row_start.saturating_add(PAGE_ROWS).min(self.row_count().max(1)), self.column_start, self.row_start.saturating_add(PAGE_ROWS) <= self.row_count()),
                        ("Prev Columns", self.row_start, self.column_start.saturating_sub(PAGE_COLUMNS).max(1), self.column_start > 1),
                        ("Next Columns", self.row_start, self.column_start.saturating_add(PAGE_COLUMNS).min(self.column_count().max(1)), self.column_start.saturating_add(PAGE_COLUMNS) <= self.column_count()),
                    ] {
                        if ui.add_enabled(enabled && valid, egui::Button::new(label)).clicked() {
                            self.row_start = row;
                            self.column_start = column;
                            action = Some(Action::Read);
                        }
                    }
                });
                if self.has_higher_axis_controls() {
                    ui.horizontal_wrapped(|ui| {
                        for (offset, &extent) in self.variable.dimensions.iter().skip(2).enumerate() {
                            ui.label(format!("Axis {}", offset + 3));
                            if extent == 0 {
                                ui.label(RichText::new("No selectable plane").color(MUTED));
                                continue;
                            }
                            if let Some(index) = self.higher_indices.get_mut(offset) {
                                if ui.add_enabled(enabled && !self.stale,
                                    egui::DragValue::new(index).range(1..=extent as u64).speed(1)
                                ).changed() {
                                    action = Some(Action::Read);
                                }
                            }
                        }
                    });
                }
                if let Some(page) = &self.typed_page {
                    if !page.selection.higher_indices.is_empty() {
                        let plane = page.selection.higher_indices.iter().enumerate().map(|(axis, index)| {
                            format!("axis {} = {}", axis + 3, index.get())
                        }).collect::<Vec<_>>().join(", ");
                        ui.label(RichText::new(format!("Displayed plane: {plane}")).small().color(MUTED));
                    }
                }
                ui.separator();
                if let Some(error) = &self.error { ui.colored_label(RED, error); }
                if self.stale {
                    ui.label(RichText::new(if self.pending { "Reading current values…" } else { "Values need refreshing before editing." }).small().color(MUTED));
                }
                let grid_height = (ui.available_height() - 105.0).max(110.0);
                egui::ScrollArea::both().id_salt("variable_grid_scroll").max_height(grid_height).auto_shrink([false, false]).show(ui, |ui| {
                    if let Some(slice) = &self.slice {
                        if slice.rows != 0 && slice.columns != 0 && !slice.cells.is_empty() {
                            egui::Grid::new("variable_cells").striped(true).min_col_width(74.0).spacing([10.0, 7.0]).show(ui, |ui| {
                                ui.label("");
                                for column in slice.column_start..slice.column_start + slice.columns {
                                    ui.label(RichText::new(column.to_string()).monospace().strong().color(MUTED));
                                }
                                ui.end_row();
                                for (row_offset, row_values) in slice.cells.iter().enumerate() {
                                    let row = slice.row_start + row_offset;
                                    ui.label(RichText::new(row.to_string()).monospace().strong().color(MUTED));
                                    for (column_offset, value) in row_values.iter().enumerate() {
                                        let column = slice.column_start + column_offset;
                                        let response = ui.selectable_label(self.selected == Some((row, column)), RichText::new(value).monospace().color(if self.stale { MUTED } else { BLUE }));
                                        if response.clicked() && !self.pending && !self.stale {
                                            self.selected = Some((row, column));
                                            self.literal.clone_from(value);
                                        }
                                    }
                                    ui.end_row();
                                }
                            });
                        } else if self.typed_page.as_ref().is_some_and(|page| page.selection.rows == 0 || page.selection.columns == 0) {
                            ui.label(RichText::new("Empty page · no elements").small().color(MUTED));
                        } else {
                            ui.label(RichText::new(&self.variable.preview).monospace());
                            ui.label(RichText::new("This value is displayed as a read-only preview.").small().color(MUTED));
                        }
                    } else {
                        ui.label(RichText::new(&self.variable.preview).monospace());
                    }
                });
                ui.separator();
                let editable = self.can_edit();
                if let Some((row, column)) = self.selected {
                    ui.horizontal(|ui| {
                        let higher = self.typed_page.as_ref().map(|page| {
                            page.selection.higher_indices.iter().map(|index| format!(",{}", index.get())).collect::<String>()
                        }).unwrap_or_default();
                        ui.label(RichText::new(format!("{}({row},{column}{higher})", self.variable.name)).monospace());
                        if editable {
                            ui.add_enabled(enabled && !self.stale, egui::TextEdit::singleline(&mut self.literal).desired_width(260.0).hint_text("Numeric or logical literal"));
                            if ui.add_enabled(enabled && !self.stale, egui::Button::new("Apply")).clicked() {
                                action = self.write_action(row, column);
                            }
                        } else { ui.label(RichText::new(&self.literal).monospace()); }
                    });
                }
                if let Some(note)=&self.native_note {ui.label(RichText::new(note).small().color(MUTED));}
                if let Some(slice) = &self.slice {
                    if slice.rows > 0 && slice.columns > 0 {
                        let total_rows = if self.variable.dimensions_known { self.row_count().to_string() } else { "—".into() };
                        let total_columns = if self.variable.dimensions_known { self.column_count().to_string() } else { "—".into() };
                        ui.label(RichText::new(format!("Rows {}–{} of {} · Columns {}–{} of {}", slice.row_start, slice.row_start + slice.rows - 1, total_rows, slice.column_start, slice.column_start + slice.columns - 1, total_columns)).small().color(MUTED));
                    }
                }
                if self.variable.dimensions.len() > 2 {
                    let message = if self.variable.dimensions.iter().skip(2).any(|&extent| extent == 0) {
                        "This array has an empty higher axis; no positive plane can be selected."
                    } else if self.typed_page.is_some() || self.native_readonly {
                        "The selected N-dimensional plane is read-only."
                    } else {
                        "The first 2D plane is shown. Typed N-D inspection is unavailable for this view."
                    };
                    ui.label(RichText::new(message).small().color(MUTED));
                }
                egui::Frame::NONE.fill(PALE).inner_margin(6).show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(if self.native_readonly { "Read-only native preview. Edit the script and Run to change values." } else if editable { "Select a cell, enter a literal, then Apply. Changes update the shared workspace." } else { "Read-only value. Use the Command Window for advanced indexing and assignment." }).small().color(MUTED));
                        if ui.add_enabled(enabled && !self.native_readonly, egui::Button::new("Display in Command Window")).clicked() { action = Some(Action::Display(self.variable.name.clone())); }
                    });
                });
            });
        (open, action)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mathlab_engine::{DecimalU64, PageSelection, SessionContext, SourceStorage, ValuePathStep};

    fn variable() -> Variable {
        Variable {
            name: "A".into(),
            class_name: "double".into(),
            dimensions: vec![90, 30],
            dimensions_known: true,
            bytes: 21600,
            bytes_known: true,
            preview: "90x30 double".into(),
        }
    }

    fn slice() -> VariableSlice {
        VariableSlice {
            name: "A".into(),
            class_name: "double".into(),
            dimensions: vec![90, 30],
            row_start: 31,
            column_start: 13,
            rows: 1,
            columns: 1,
            cells: vec![vec!["12.5".into()]],
            editable: true,
        }
    }

    fn token(revision: u64) -> BindingToken {
        BindingToken {
            context: SessionContext {
                worker_id: "editor-worker".into(),
                generation: 2,
                invocation: 3,
                revision,
            },
            name: "A".into(),
        }
    }

    fn page(revision: u64) -> VariablePage {
        VariablePage::Checked(BoundVariableSlice {
            token: token(revision),
            slice: slice(),
        })
    }

    #[test]
    fn obsolete_worker_reply_does_not_replace_current_page() {
        let mut editor = VariableEditor::new(variable());
        editor.begin(7);
        editor.accept(6, Ok(page(1)));
        assert!(editor.pending);
        assert!(editor.slice.is_none());
        assert!(editor.token.is_none());
        editor.accept(7, Ok(page(2)));
        assert!(!editor.pending);
        assert!(!editor.stale);
        assert_eq!((editor.row_start, editor.column_start), (31, 13));
        assert_eq!(editor.token, Some(token(2)));
        editor.accept(6, Ok(page(1)));
        assert_eq!(editor.token, Some(token(2)));
    }

    #[test]
    fn workspace_refresh_clamps_page_and_removes_stale_edit() {
        let mut editor = VariableEditor::new(variable());
        editor.begin(1);
        editor.accept(1, Ok(page(1)));
        editor.row_start = 80;
        editor.column_start = 25;
        editor.selected = Some((80, 25));
        editor.literal = "2".into();
        let mut smaller = variable();
        smaller.dimensions = vec![2, 3];
        editor.invalidate(Some(smaller));
        assert_eq!((editor.row_start, editor.column_start), (2, 3));
        assert!(editor.stale);
        assert!(editor.token.is_none());
        assert!(editor.selected.is_none());
        assert!(editor.literal.is_empty());
        editor.invalidate(None);
        assert!(editor.error.as_ref().unwrap().contains("no longer"));
    }

    #[test]
    fn write_uses_the_exact_accepted_page_token_and_failure_cannot_retry_it() {
        let mut editor = VariableEditor::new(variable());
        assert!(editor.write_action(31, 13).is_none());
        editor.begin(1);
        editor.accept(1, Ok(page(9)));
        editor.literal = "42".into();
        match editor.write_action(31, 13).unwrap() {
            Action::Write {
                token: accepted,
                row,
                column,
                literal,
            } => {
                assert_eq!(accepted, token(9));
                assert_eq!((row, column, literal.as_str()), (31, 13, "42"));
            }
            _ => panic!("expected a checked write"),
        }
        editor.begin(2);
        assert!(editor.token.is_none());
        assert!(editor.write_action(31, 13).is_none());
        editor.accept(
            2,
            Err("The workspace binding changed. Refresh this variable.".into()),
        );
        assert!(editor.stale);
        assert!(!editor.pending);
        assert!(editor.error.as_ref().unwrap().contains("binding changed"));
        assert!(editor.write_action(31, 13).is_none());
        assert_eq!(editor.slice.as_ref().unwrap().cells[0][0], "12.5");
        editor.accept(1, Ok(page(9)));
        assert!(
            editor.token.is_none(),
            "late page must not reauthorize editing"
        );
    }

    #[test]
    fn legacy_preview_is_read_only_even_if_its_payload_claims_editable() {
        let mut editor = VariableEditor::new(variable());
        editor.begin(1);
        editor.accept(1, Ok(VariablePage::Preview(slice())));
        assert!(!editor.stale);
        assert!(editor.token.is_none());
        assert!(!editor.slice.as_ref().unwrap().editable);
        assert!(editor.write_action(31, 13).is_none());
    }

    #[test]
    fn mismatched_page_binding_does_not_authorize_the_selected_variable() {
        let mut editor = VariableEditor::new(variable());
        let mut wrong = token(1);
        wrong.name = "B".into();
        editor.begin(1);
        editor.accept(
            1,
            Ok(VariablePage::Checked(BoundVariableSlice {
                token: wrong,
                slice: slice(),
            })),
        );
        assert!(editor.stale);
        assert!(editor.token.is_none());
        assert!(editor.error.as_ref().unwrap().contains("selection changed"));
    }

    #[test]
    fn unknown_metadata_stays_unavailable_until_a_page_establishes_its_shape() {
        let mut metadata = variable();
        metadata.dimensions.clear();
        metadata.dimensions_known = false;
        metadata.bytes = 0;
        metadata.bytes_known = false;
        let mut editor = VariableEditor::new(metadata.clone());
        assert_eq!(variable_size(&editor.variable), "—");
        assert_eq!(variable_bytes(&editor.variable), "bytes unavailable");
        editor.invalidate(Some(metadata));
        assert!(!editor.variable.dimensions_known);
        assert!(!editor.variable.bytes_known);
        editor.begin(1);
        editor.accept(1, Ok(page(1)));
        assert!(editor.variable.dimensions_known);
        assert_eq!(variable_size(&editor.variable), "90×30");
        assert!(!editor.variable.bytes_known);
        assert_eq!(variable_bytes(&editor.variable), "bytes unavailable");
    }

    fn typed_page(
        shape: &[u64],
        higher: &[u64],
        class: NumericClass,
        real: NumericComponents,
    ) -> ValuePage {
        ValuePage {
            token: token(9),
            path: Vec::new(),
            class,
            source_shape: shape.iter().copied().map(DecimalU64::new).collect(),
            source_storage: SourceStorage::Dense,
            complex: false,
            selection: PageSelection {
                row_start: DecimalU64::new(1),
                column_start: DecimalU64::new(1),
                rows: shape[0].min(PAGE_ROWS as u64) as u32,
                columns: shape[1].min(PAGE_COLUMNS as u64) as u32,
                higher_indices: higher.iter().copied().map(DecimalU64::new).collect(),
            },
            data: PageData::Dense { real, imag: None },
        }
    }

    fn typed_result(page: ValuePage) -> VariablePage {
        VariablePage::Typed {
            page: Box::new(page),
            metadata: None,
        }
    }

    fn nd_page(plane: u64) -> ValuePage {
        let start = 1 + (plane - 1) * 12;
        typed_page(
            &[3, 4, 5],
            &[plane],
            NumericClass::Double,
            NumericComponents::Double((start..start + 12).map(|value| value as f64).collect()),
        )
    }

    #[test]
    fn typed_nd_plane_four_uses_column_major_values_and_stays_read_only() {
        let mut metadata = variable();
        metadata.dimensions = vec![3, 4, 5];
        let mut editor = VariableEditor::new(metadata);
        editor.higher_indices = vec![4];
        editor.begin_read(1, Some(token(9)));
        editor.accept(1, Ok(typed_result(nd_page(4))));
        assert!(!editor.stale);
        assert_eq!(
            editor.slice.as_ref().unwrap().cells,
            [
                vec!["37", "40", "43", "46"],
                vec!["38", "41", "44", "47"],
                vec!["39", "42", "45", "48"],
            ]
        );
        assert_eq!(editor.variable.dimensions, [3, 4, 5]);
        assert_eq!(editor.higher_indices, [4]);
        assert_eq!(
            editor.typed_page.as_ref().unwrap().selection.higher_indices[0].get(),
            4
        );
        assert!(
            editor.write_action(2, 3).is_none(),
            "N-D coordinates cannot use the root 2-D writer"
        );
    }

    #[test]
    fn typed_display_preserves_exact_uint64_and_signed_extrema() {
        let unsigned = typed_page(
            &[1, 3],
            &[],
            NumericClass::Uint64,
            NumericComponents::Uint64(vec![9_007_199_254_741_009, u64::MAX, 0]),
        );
        assert_eq!(
            typed_projection(&unsigned).unwrap().cells[0],
            ["9007199254741009", "18446744073709551615", "0",]
        );
        let signed = typed_page(
            &[1, 2],
            &[],
            NumericClass::Int64,
            NumericComponents::Int64(vec![i64::MIN, i64::MAX]),
        );
        assert_eq!(
            typed_projection(&signed).unwrap().cells[0],
            ["-9223372036854775808", "9223372036854775807",]
        );
    }

    #[test]
    fn typed_float_and_complex_display_keeps_nonfinite_values_and_signed_zero() {
        let mut page = typed_page(
            &[1, 3],
            &[],
            NumericClass::Double,
            NumericComponents::Double(vec![-0.0, f64::INFINITY, f64::NAN]),
        );
        page.complex = true;
        page.data = PageData::Dense {
            real: NumericComponents::Double(vec![-0.0, f64::INFINITY, f64::NAN]),
            imag: Some(NumericComponents::Double(vec![
                -0.0,
                f64::NEG_INFINITY,
                0.0,
            ])),
        };
        assert_eq!(
            typed_projection(&page).unwrap().cells[0],
            ["-0-0i", "Inf-Infi", "NaN+0i"]
        );
        let single = typed_page(
            &[1, 2],
            &[],
            NumericClass::Single,
            NumericComponents::Single(vec![0.1_f32, -0.0]),
        );
        assert_eq!(typed_projection(&single).unwrap().cells[0], ["0.1", "-0"]);
    }

    #[test]
    fn sparse_projection_formats_only_the_page_and_preserves_implicit_zeros() {
        let mut page = typed_page(
            &[2_000_000_000, 2_000_000_000],
            &[],
            NumericClass::Double,
            NumericComponents::Double(Vec::new()),
        );
        page.source_storage = SourceStorage::Sparse;
        page.selection.rows = 3;
        page.selection.columns = 4;
        page.selection.row_start = DecimalU64::new(1_000_000);
        page.selection.column_start = DecimalU64::new(2_000_000);
        page.data = PageData::SparseCoo {
            rows: vec![2],
            columns: vec![3],
            real: NumericComponents::Double(vec![5.0]),
            imag: None,
        };
        let display = typed_projection(&page).unwrap();
        assert_eq!(display.cells.len(), 3);
        assert_eq!(display.cells[0], ["0", "0", "0", "0"]);
        assert_eq!(display.cells[1], ["0", "0", "5", "0"]);
        assert_eq!(display.dimensions, [2_000_000_000, 2_000_000_000]);
        assert!(matches!(&page.data, PageData::SparseCoo { real, .. } if real.len() == 1));
        let mut logical = typed_page(
            &[2, 2],
            &[],
            NumericClass::Logical,
            NumericComponents::Logical(Vec::new()),
        );
        logical.source_storage = SourceStorage::Sparse;
        logical.data = PageData::SparseCoo {
            rows: vec![1],
            columns: vec![2],
            real: NumericComponents::Logical(vec![true]),
            imag: None,
        };
        assert_eq!(
            typed_projection(&logical).unwrap().cells,
            [vec!["false", "true"], vec!["false", "false"]]
        );
    }

    #[test]
    fn empty_typed_nd_page_retains_shape_and_does_not_invent_metadata_bytes() {
        let page = typed_page(
            &[0, 3, 2],
            &[2],
            NumericClass::Double,
            NumericComponents::Double(Vec::new()),
        );
        let mut metadata = variable();
        metadata.dimensions.clear();
        metadata.dimensions_known = false;
        metadata.bytes = 0;
        metadata.bytes_known = false;
        let mut editor = VariableEditor::new(metadata.clone());
        editor.higher_indices = vec![2];
        editor.begin_read(1, Some(token(9)));
        editor.accept(
            1,
            Ok(VariablePage::Typed {
                page: Box::new(page),
                metadata: Some(metadata),
            }),
        );
        assert!(!editor.stale);
        assert_eq!(editor.variable.dimensions, [0, 3, 2]);
        assert!(editor.variable.dimensions_known);
        assert!(!editor.variable.bytes_known);
        assert!(editor.slice.as_ref().unwrap().cells.is_empty());
        assert_eq!(editor.typed_page.as_ref().unwrap().selection.columns, 3);
        assert!(editor.write_action(1, 1).is_none());
    }

    #[test]
    fn late_or_wrong_plane_replies_cannot_replace_the_accepted_selection() {
        let mut metadata = variable();
        metadata.dimensions = vec![3, 4, 5];
        let mut editor = VariableEditor::new(metadata);
        editor.higher_indices = vec![2];
        editor.begin_read(10, Some(token(9)));
        editor.higher_indices = vec![4];
        editor.begin_read(11, Some(token(9)));
        editor.accept(10, Ok(typed_result(nd_page(2))));
        assert!(editor.pending);
        assert!(editor.token.is_none());
        editor.accept(11, Ok(typed_result(nd_page(4))));
        assert_eq!(editor.higher_indices, [4]);
        assert_eq!(editor.slice.as_ref().unwrap().cells[1][2], "44");
        editor.begin_read(12, Some(token(9)));
        editor.accept(12, Ok(typed_result(nd_page(2))));
        assert!(editor.stale);
        assert!(editor.token.is_none());
        assert!(editor
            .error
            .as_ref()
            .unwrap()
            .contains("requested binding or plane"));
        assert_eq!(
            editor.typed_page.as_ref().unwrap().selection.higher_indices[0].get(),
            4
        );
    }

    #[test]
    fn root_two_dimensional_typed_pages_keep_scalar_writes_but_paths_do_not() {
        let page = typed_page(
            &[1, 1],
            &[],
            NumericClass::Double,
            NumericComponents::Double(vec![3.0]),
        );
        let mut editor = VariableEditor::new(variable());
        editor.begin_read(1, Some(token(9)));
        editor.accept(1, Ok(typed_result(page.clone())));
        assert!(
            matches!(editor.write_action(1, 1), Some(Action::Write { token: accepted, .. }) if accepted == token(9))
        );
        let mut leaf = page;
        leaf.path.push(ValuePathStep::Cell {
            indices: vec![DecimalU64::new(1), DecimalU64::new(1)],
        });
        editor.slice = Some(typed_projection(&leaf).unwrap());
        editor.typed_page = Some(leaf);
        assert!(
            editor.write_action(1, 1).is_none(),
            "nested coordinates cannot mutate the root binding"
        );
    }

    #[test]
    fn typed_failure_retains_only_stale_display_and_cannot_downgrade_to_preview() {
        let accepted = typed_page(
            &[1, 1],
            &[],
            NumericClass::Double,
            NumericComponents::Double(vec![3.0]),
        );
        let mut editor = VariableEditor::new(variable());
        editor.begin_read(1, Some(token(9)));
        editor.accept(1, Ok(typed_result(accepted)));
        assert!(editor.can_edit());
        editor.begin_read(2, Some(token(9)));
        editor.accept(2, Err("Typed page binding is stale.".into()));
        assert!(editor.stale);
        assert!(editor.token.is_none());
        assert!(!editor.can_edit());
        assert_eq!(editor.slice.as_ref().unwrap().cells[0][0], "3");
        assert_eq!(editor.typed_page.as_ref().unwrap().token, token(9));

        editor.begin_read(3, Some(token(9)));
        editor.accept(3, Ok(VariablePage::Preview(slice())));
        assert!(editor.stale);
        assert!(editor.token.is_none());
        assert!(!editor.can_edit());
        assert!(editor
            .error
            .as_ref()
            .unwrap()
            .contains("unconditional preview"));
        assert_eq!(editor.slice.as_ref().unwrap().cells[0][0], "3");
    }

    #[test]
    fn malformed_empty_page_cannot_allocate_an_unbounded_display_extent() {
        let mut page = typed_page(
            &[u64::from(u32::MAX), 0],
            &[],
            NumericClass::Double,
            NumericComponents::Double(Vec::new()),
        );
        page.selection.rows = u32::MAX;
        page.selection.columns = 0;
        assert!(typed_projection(&page)
            .unwrap_err()
            .contains("bounded display extent"));
    }

    #[test]
    fn legacy_nd_checked_page_cannot_use_the_root_two_dimensional_writer() {
        let mut metadata = variable();
        metadata.dimensions = vec![90, 30, 2];
        let mut editor = VariableEditor::new(metadata);
        let mut first_plane = slice();
        first_plane.dimensions.push(2);
        assert!(first_plane.editable);
        editor.begin_read(1, Some(token(9)));
        editor.accept(
            1,
            Ok(VariablePage::Checked(BoundVariableSlice {
                token: token(9),
                slice: first_plane,
            })),
        );
        assert!(!editor.stale);
        assert!(editor.token.is_some());
        assert!(editor.write_action(31, 13).is_none());
    }

    #[test]
    fn fresh_metadata_after_failed_read_clamps_the_draft_without_accepting_values() {
        let mut old = variable();
        old.dimensions = vec![3, 4, 5];
        let mut editor = VariableEditor::new(old);
        editor.higher_indices = vec![4];
        editor.begin_read(1, Some(token(9)));
        editor.accept(1, Ok(typed_result(nd_page(4))));
        assert!(editor.typed_page.is_some());
        editor.row_start = 80;
        editor.column_start = 25;
        editor.selected = Some((2, 3));
        editor.literal = "44".into();
        let mut fresh = variable();
        fresh.dimensions = vec![2, 3, 2, 4];
        fresh.bytes = 0;
        fresh.bytes_known = false;
        editor.begin_read(2, None);
        editor.accept(
            2,
            Ok(VariablePage::MetadataOnly {
                variable: fresh,
                diagnostic: "The selected page no longer matches the array rank.".into(),
            }),
        );
        assert_eq!(editor.variable.dimensions, [2, 3, 2, 4]);
        assert!(!editor.variable.bytes_known);
        assert_eq!((editor.row_start, editor.column_start), (2, 3));
        assert_eq!(editor.higher_indices, [2, 1]);
        assert!(editor.stale);
        assert!(!editor.pending);
        assert!(editor.token.is_none());
        assert!(editor.slice.is_none());
        assert!(editor.typed_page.is_none());
        assert!(editor.selected.is_none());
        assert!(editor.literal.is_empty());
        assert!(editor.error.as_ref().unwrap().contains("array rank"));
        assert!(editor.write_action(2, 3).is_none());

        // Only another explicit user read can accept values at the corrected
        // draft. A delayed reply from the obsolete request cannot restore them.
        editor.accept(1, Ok(typed_result(nd_page(4))));
        assert!(editor.slice.is_none());
        let mut corrected = typed_page(
            &[2, 3, 2, 4],
            &[2, 1],
            NumericClass::Double,
            NumericComponents::Double(vec![12.0]),
        );
        corrected.token = token(10);
        corrected.selection.row_start = DecimalU64::new(2);
        corrected.selection.column_start = DecimalU64::new(3);
        corrected.selection.rows = 1;
        corrected.selection.columns = 1;
        editor.begin_read(3, None);
        editor.accept(3, Ok(typed_result(corrected)));
        assert!(!editor.stale);
        assert_eq!(editor.token, Some(token(10)));
        assert_eq!(editor.slice.as_ref().unwrap().cells[0][0], "12");
        assert!(!editor.variable.bytes_known);
        assert!(editor.write_action(2, 3).is_none());
    }

    #[test]
    fn metadata_only_result_cannot_replace_another_selection_or_a_checked_read() {
        let mut editor = VariableEditor::new(variable());
        editor.begin_read(1, None);
        let mut wrong = variable();
        wrong.name = "B".into();
        editor.accept(
            1,
            Ok(VariablePage::MetadataOnly {
                variable: wrong,
                diagnostic: "failed".into(),
            }),
        );
        assert_eq!(editor.variable.name, "A");
        assert!(editor.stale);
        assert!(editor.token.is_none());
        assert!(editor.error.as_ref().unwrap().contains("does not match"));

        editor.begin_read(2, Some(token(9)));
        let mut wrong_shape = variable();
        wrong_shape.dimensions = vec![1, 1];
        editor.accept(
            2,
            Ok(VariablePage::MetadataOnly {
                variable: wrong_shape,
                diagnostic: "failed".into(),
            }),
        );
        assert_eq!(editor.variable.dimensions, [90, 30]);
        assert!(editor.stale);
        assert!(editor.token.is_none());
        assert!(editor.error.as_ref().unwrap().contains("does not match"));
    }

    #[test]
    fn transaction_quarantine_blocks_scalar_apply_but_survives_a_successful_read() {
        let mut editor = VariableEditor::new(variable());
        editor.begin(1);
        editor.accept(1, Ok(page(9)));
        assert!(editor.write_action(31, 13).is_some());
        editor.writes_blocked = true;
        assert!(editor.write_action(31, 13).is_none());
        editor.begin_read(2, Some(token(9)));
        editor.accept(2, Ok(page(9)));
        assert!(!editor.stale && editor.token.is_some());
        assert!(
            editor.write_action(31, 13).is_none(),
            "a pure read cannot clear transaction quarantine"
        );
        editor.writes_blocked = false;
        assert!(editor.write_action(31, 13).is_some());
    }
    #[test]
    fn native_plane_widgets_reject_wrong_response_coordinates_and_empty_axes_stay_readonly(){
        let mut editor=VariableEditor::new(variable());editor.variable.dimensions=vec![2,2,2];editor.higher_indices=vec![1];
        assert!(!editor.has_higher_axis_controls());editor.native_readonly=true;assert!(editor.has_higher_axis_controls());
        let mut page=crate::rust_worker::Page{token:crate::rust_worker::PageToken{key:crate::rust_worker::JobKey{generation:1,job:1},revision:1,binding:editor.variable.name.clone()},dimensions:vec![2,2,2],row:0,column:0,higher:vec![0],rows:2,columns:2,class_name:editor.variable.class_name.clone(),storage:"dense",complex:false,cells:vec![vec![crate::rust_worker::Cell{text:"1".into(),stored:true},crate::rust_worker::Cell{text:"3".into(),stored:true}],vec![crate::rust_worker::Cell{text:"2".into(),stored:true},crate::rust_worker::Cell{text:"4".into(),stored:true}]],object:None};
        editor.begin_read(1,None);assert!(editor.accept_native(1,&page));assert!(editor.native_note.as_ref().unwrap().contains("axis 3 = 1"));assert!(!editor.can_edit());
        editor.higher_indices[0]=2;editor.begin_read(2,None);
        assert!(!editor.accept_native(1,&page));assert!(!editor.accept_native(2,&page));assert!(editor.pending);assert_eq!(editor.higher_indices,[2]);
        page.higher[0]=1;page.cells[0][0].text="5".into();assert!(editor.accept_native(2,&page));assert_eq!(editor.slice.as_ref().unwrap().cells[0][0],"5");assert!(editor.native_note.as_ref().unwrap().contains("axis 3 = 2"));assert!(!editor.can_edit());assert!(editor.token.is_none());
        editor.variable.dimensions=vec![2,3,0];editor.higher_indices=vec![1];assert!(editor.show_native_empty_plane());assert!(editor.has_higher_axis_controls());assert!(editor.slice.is_none());assert!(!editor.pending);assert!(!editor.can_edit());assert!(editor.native_note.as_ref().unwrap().contains("no selectable plane"));
        editor.native_readonly=false;assert!(!editor.has_higher_axis_controls());assert!(!editor.show_native_empty_plane());
        editor.typed_page=Some(typed_page(&[1,1,2],&[1],NumericClass::Double,NumericComponents::Double(vec![3.])));editor.variable.dimensions=vec![1,1,2];assert!(editor.has_higher_axis_controls(),"existing Octave typed selector remains enabled");
    }

}

impl VariableEditor {
    pub fn clear_native_page(&mut self){self.slice=None;self.typed_page=None;self.native_note=None;self.token=None;}
    pub(crate) fn has_higher_axis_controls(&self)->bool{
        (self.typed_page.is_some() || (self.native_readonly && self.variable.dimensions_known)) && self.variable.dimensions.len()>2
    }
    pub(crate) fn show_native_empty_plane(&mut self)->bool{
        if !self.native_readonly || !self.variable.dimensions_known || !self.variable.dimensions.iter().skip(2).any(|&n|n==0){return false}
        self.clear_native_page();self.pending=false;self.pending_read=None;self.stale=false;self.error=None;self.writes_blocked=true;
        self.native_note=Some("Empty higher axis: no selectable plane; read-only shape metadata, no numeric cells.".into());true
    }
    pub fn accept_native(&mut self,request_id:u64,page:&crate::rust_worker::Page)->bool{
        if self.request_id!=request_id || self.variable.name!=page.token.binding{return false}
        if page.rows>crate::rust_worker::MAX_PAGE_CELLS || page.columns>crate::rust_worker::MAX_PAGE_CELLS || page.rows.checked_mul(page.columns).is_none_or(|n|n>crate::rust_worker::MAX_PAGE_CELLS) || page.higher.len()>6{return false}
        let Some(expected)=self.pending_read.as_ref()else{return false};
        // Compare the exact sent plane, not the token alone: one binding/revision has many planes.
        if expected.token.is_some() || page.row.checked_add(1)!=Some(expected.row_start) || page.column.checked_add(1)!=Some(expected.column_start) || page.higher.len()!=expected.higher_indices.len() || !page.higher.iter().zip(&expected.higher_indices).all(|(&zero,&one)|zero.checked_add(1)==Some(one)){return false}
        if page.object.is_none() && (page.dimensions!=self.variable.dimensions || page.class_name!=self.variable.class_name){return false}
        self.pending=false;self.pending_read=None;self.stale=false;self.token=None;self.typed_page=None;self.native_readonly=true;self.writes_blocked=true;self.error=None;
        self.higher_indices=page.higher.iter().map(|x|x+1).collect();
        if let Some(object)=&page.object{self.slice=None;self.native_note=Some(object.clone());self.variable.dimensions_known=false;return true}
        self.variable.dimensions=page.dimensions.clone();self.variable.dimensions_known=true;self.variable.class_name=page.class_name.clone();
        self.row_start=page.row+1;self.column_start=page.column+1;
        let plane=page.higher.iter().enumerate().map(|(axis,index)|format!("axis {} = {}",axis+3,index+1)).collect::<Vec<_>>().join(", ");
        self.native_note=Some(format!("{}{}; read-only exact selected plane{}. CSC '0 (implicit)' marks absence of a stored entry.",page.storage,if page.complex{" complex"}else{""},if plane.is_empty(){String::new()}else{format!(": {plane}")}));
        self.slice=Some(VariableSlice{name:page.token.binding.clone(),class_name:page.class_name.clone(),dimensions:page.dimensions.clone(),row_start:self.row_start,column_start:self.column_start,rows:page.rows,columns:page.columns,cells:page.cells.iter().map(|r|r.iter().map(|c|if page.storage=="csc" && !c.stored{"0 (implicit)".into()}else{c.text.clone()}).collect()).collect(),editable:false});
        true
    }
}
