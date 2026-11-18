//! Ergonomic `WinUI 3` dark `Fluent` control constructors, layout helpers, and `WriteableBitmap` blitting.

use windows_core::{HSTRING, Interface};

use crate::bindings::Microsoft::UI::Text::FontWeights;
use crate::bindings::Microsoft::UI::Xaml::Controls::{
    Border, Button, CheckBox, ColumnDefinition, ComboBox, Grid, InfoBar, InfoBarSeverity,
    Orientation, PasswordBox, ProgressBar, RowDefinition, ScrollBarVisibility, ScrollMode,
    ScrollViewer, StackPanel, TextBlock, TextBox,
};
use crate::bindings::Microsoft::UI::Xaml::Media::Imaging::WriteableBitmap;
use crate::bindings::Microsoft::UI::Xaml::Media::{Brush, SolidColorBrush};
use crate::bindings::Microsoft::UI::Xaml::{
    CornerRadius, FrameworkElement, GridLength, GridUnitType, HorizontalAlignment, TextWrapping,
    Thickness, VerticalAlignment,
};
use crate::bindings::Windows::UI::Color;
use crate::interop::raw_com::IBufferByteAccess;

/// Canvas background (`#181818`).
pub const BG_CANVAS: Color = argb(255, 24, 24, 24);
/// Surface background (`#202020`).
pub const BG_SURFACE: Color = argb(255, 32, 32, 32);
/// Card background (`#262626`).
pub const BG_CARD: Color = argb(255, 38, 38, 38);
/// Elevated surface (`#323232`).
pub const BG_ELEVATED: Color = argb(255, 50, 50, 50);
/// Subtle 1px dark border (`#383838`).
pub const BORDER_SUBTLE: Color = argb(255, 56, 56, 56);
/// Primary crisp white text (`#F3F3F3`).
pub const TEXT_PRIMARY: Color = argb(255, 243, 243, 243);
/// Muted secondary text (`#A6A6A6`).
pub const TEXT_SECONDARY: Color = argb(255, 166, 166, 166);
/// Fluent Sky Blue accent (`#4CC2FF`).
pub const ACCENT_BLUE: Color = argb(255, 76, 194, 255);
/// Deep readable text over accent fill (`#0D151A`).
pub const ACCENT_TEXT: Color = argb(255, 13, 21, 26);
/// Danger/error red (`#FF6B6B`).
pub const ACCENT_RED: Color = argb(255, 255, 107, 107);
/// Warning amber (`#F7B955`).
pub const ACCENT_AMBER: Color = argb(255, 247, 185, 85);
/// Success green (`#6CCB5F`).
pub const ACCENT_GREEN: Color = argb(255, 108, 203, 95);

/// Constructs a `Windows::UI::Color` from 8-bit `ARGB` channels.
#[must_use]
pub const fn argb(a: u8, r: u8, g: u8, b: u8) -> Color {
    Color {
        A: a,
        R: r,
        G: g,
        B: b,
    }
}

/// Creates a `SolidColorBrush` with the given `Color`.
///
/// # Errors
///
/// Returns an error if `WinUI` XAML activation fails.
pub fn brush(color: Color) -> windows_core::Result<SolidColorBrush> {
    SolidColorBrush::CreateInstanceWithColor(color)
}

/// Converts a `SolidColorBrush` to a generic XAML `Brush`.
///
/// # Errors
///
/// Returns an error if casting fails.
pub fn brush_base(color: Color) -> windows_core::Result<Brush> {
    brush(color)?.cast()
}

/// Constructs symmetric `Thickness(v, v, v, v)`.
#[must_use]
pub const fn uniform(v: f64) -> Thickness {
    Thickness {
        Left: v,
        Top: v,
        Right: v,
        Bottom: v,
    }
}

/// Constructs `Thickness(h, v, h, v)`.
#[must_use]
pub const fn hv(h: f64, v: f64) -> Thickness {
    Thickness {
        Left: h,
        Top: v,
        Right: h,
        Bottom: v,
    }
}

/// Constructs explicit `Thickness(left, top, right, bottom)`.
#[must_use]
pub const fn pad(left: f64, top: f64, right: f64, bottom: f64) -> Thickness {
    Thickness {
        Left: left,
        Top: top,
        Right: right,
        Bottom: bottom,
    }
}

/// Constructs uniform `CornerRadius(r)`.
#[must_use]
pub const fn radius(r: f64) -> CornerRadius {
    CornerRadius {
        TopLeft: r,
        TopRight: r,
        BottomRight: r,
        BottomLeft: r,
    }
}

/// `GridLength::Auto`.
#[must_use]
pub const fn gl_auto() -> GridLength {
    GridLength {
        Value: 1.0,
        GridUnitType: GridUnitType::Auto,
    }
}

/// `GridLength::Pixel(px)`.
#[must_use]
pub const fn gl_px(px: f64) -> GridLength {
    GridLength {
        Value: px,
        GridUnitType: GridUnitType::Pixel,
    }
}

/// `GridLength::Star(weight)`.
#[must_use]
pub const fn gl_star(weight: f64) -> GridLength {
    GridLength {
        Value: weight,
        GridUnitType: GridUnitType::Star,
    }
}

/// Builds a `Grid` populated with `RowDefinition` lengths.
///
/// # Errors
///
/// Returns an error if any `WinUI` call fails.
pub fn grid_rows(rows: &[GridLength]) -> windows_core::Result<Grid> {
    let grid = Grid::new()?;
    let defs = grid.RowDefinitions()?;
    for &len in rows {
        let rd = RowDefinition::new()?;
        rd.SetHeight(len)?;
        defs.Append(&rd)?;
    }
    Ok(grid)
}

/// Appends column definitions to an existing `Grid`.
///
/// # Errors
///
/// Returns an error if adding column definitions fails.
pub fn add_grid_cols(grid: &Grid, cols: &[GridLength]) -> windows_core::Result<()> {
    let defs = grid.ColumnDefinitions()?;
    for &len in cols {
        let cd = ColumnDefinition::new()?;
        cd.SetWidth(len)?;
        defs.Append(&cd)?;
    }
    Ok(())
}

/// Positions an element cleanly inside a `Grid` (`row`, `col`, optional spans).
///
/// # Errors
///
/// Returns an error if casting to `FrameworkElement` or `Grid::Set*` attached property calls fail.
pub fn place_in_grid<T>(
    element: &T,
    row: i32,
    col: i32,
    row_span: i32,
    col_span: i32,
) -> windows_core::Result<()>
where
    T: Interface,
{
    let fe: FrameworkElement = element.cast()?;
    Grid::SetRow(&fe, row)?;
    Grid::SetColumn(&fe, col)?;
    if row_span > 1 {
        Grid::SetRowSpan(&fe, row_span)?;
    }
    if col_span > 1 {
        Grid::SetColumnSpan(&fe, col_span)?;
    }
    Ok(())
}

/// Creates a vertical `StackPanel` with `spacing`.
///
/// # Errors
///
/// Returns an error if `StackPanel` instantiation fails.
pub fn stack_v(spacing: f64) -> windows_core::Result<StackPanel> {
    let panel = StackPanel::new()?;
    panel.SetOrientation(Orientation::Vertical)?;
    panel.SetSpacing(spacing)?;
    Ok(panel)
}

/// Creates a horizontal `StackPanel` with `spacing`.
///
/// # Errors
///
/// Returns an error if `StackPanel` instantiation fails.
pub fn stack_h(spacing: f64) -> windows_core::Result<StackPanel> {
    let panel = StackPanel::new()?;
    panel.SetOrientation(Orientation::Horizontal)?;
    panel.SetSpacing(spacing)?;
    Ok(panel)
}

/// Creates a styled dark card `Border` (`#262626` background, `#383838` border, 8px radius).
///
/// # Errors
///
/// Returns an error if `Border` or brush construction fails.
pub fn card_border(padding: Thickness) -> windows_core::Result<Border> {
    let border = Border::new()?;
    border.SetBackground(&brush_base(BG_CARD)?)?;
    border.SetBorderBrush(&brush_base(BORDER_SUBTLE)?)?;
    border.SetBorderThickness(uniform(1.0))?;
    border.SetCornerRadius(radius(8.0))?;
    border.SetPadding(padding)?;
    Ok(border)
}

/// Creates a styled `TextBlock`.
///
/// # Errors
///
/// Returns an error if `TextBlock` construction fails.
pub fn text_block(
    text: &str,
    font_size: f64,
    semi_bold: bool,
    color: Color,
) -> windows_core::Result<TextBlock> {
    let tb = TextBlock::new()?;
    tb.SetText(&HSTRING::from(text))?;
    tb.SetFontSize(font_size)?;
    if semi_bold {
        tb.SetFontWeight(FontWeights::SemiBold()?)?;
    }
    tb.SetForeground(&brush_base(color)?)?;
    tb.SetTextWrapping(TextWrapping::Wrap)?;
    tb.SetVerticalAlignment(VerticalAlignment::Center)?;
    Ok(tb)
}

/// Creates a dark or accent-filled `Button`.
///
/// # Errors
///
/// Returns an error if `Button` or brush creation fails.
pub fn button(label: &str, primary: bool) -> windows_core::Result<Button> {
    let btn = Button::new()?;
    let label_color = if primary { ACCENT_TEXT } else { TEXT_PRIMARY };
    let label_tb = text_block(label, 13.5, primary, label_color)?;
    label_tb.SetTextWrapping(TextWrapping::NoWrap)?;
    btn.SetContent(&label_tb)?;
    btn.SetPadding(hv(14.0, 7.0))?;
    btn.SetCornerRadius(radius(6.0))?;
    if primary {
        btn.SetBackground(&brush_base(ACCENT_BLUE)?)?;
        btn.SetBorderBrush(&brush_base(ACCENT_BLUE)?)?;
    } else {
        btn.SetBackground(&brush_base(BG_ELEVATED)?)?;
        btn.SetBorderBrush(&brush_base(BORDER_SUBTLE)?)?;
    }
    Ok(btn)
}

/// Creates a `TextBox` with placeholder text and optional read-only state.
///
/// # Errors
///
/// Returns an error if `TextBox` creation fails.
pub fn text_box(placeholder: &str, read_only: bool) -> windows_core::Result<TextBox> {
    let tb = TextBox::new()?;
    if !placeholder.is_empty() {
        tb.SetPlaceholderText(&HSTRING::from(placeholder))?;
    }
    tb.SetIsReadOnly(read_only)?;
    Ok(tb)
}

/// Creates a `PasswordBox` with placeholder text.
///
/// # Errors
///
/// Returns an error if `PasswordBox` creation fails.
pub fn password_box(placeholder: &str) -> windows_core::Result<PasswordBox> {
    let pb = PasswordBox::new()?;
    if !placeholder.is_empty() {
        pb.SetPlaceholderText(&HSTRING::from(placeholder))?;
    }
    Ok(pb)
}

/// Creates a `CheckBox` with text label and initial checked state.
///
/// # Errors
///
/// Returns an error if `CheckBox` creation fails.
pub fn check_box(label: &str, checked: bool) -> windows_core::Result<CheckBox> {
    let cb = CheckBox::new()?;
    let tb = text_block(label, 13.0, false, TEXT_PRIMARY)?;
    cb.SetContent(&tb)?;
    cb.SetIsChecked(Some(checked))?;
    Ok(cb)
}

/// Creates a `ComboBox` populated with `TextBlock` string items.
///
/// # Errors
///
/// Returns an error if `ComboBox` creation or item insertion fails.
pub fn combo_box(items: &[&str], selected_index: i32) -> windows_core::Result<ComboBox> {
    let combo = ComboBox::new()?;
    let vec = combo.Items()?;
    for item in items {
        let tb = text_block(item, 13.0, false, TEXT_PRIMARY)?;
        vec.Append(&tb)?;
    }
    if !items.is_empty() && selected_index >= 0 {
        combo.SetSelectedIndex(selected_index)?;
    }
    combo.SetHorizontalAlignment(HorizontalAlignment::Stretch)?;
    Ok(combo)
}

/// Creates a collapsible non-modal `InfoBar` (`IsOpen = false` initially).
///
/// # Errors
///
/// Returns an error if `InfoBar` creation fails.
pub fn info_bar() -> windows_core::Result<InfoBar> {
    let bar = InfoBar::new()?;
    bar.SetIsOpen(false)?;
    bar.SetIsClosable(true)?;
    Ok(bar)
}

/// Updates and reveals an `InfoBar` with `severity`, `title`, and `message`.
pub fn show_info(bar: &InfoBar, severity: InfoBarSeverity, title: &str, message: &str) {
    let _ = bar.SetSeverity(severity);
    let _ = bar.SetTitle(&HSTRING::from(title));
    let _ = bar.SetMessage(&HSTRING::from(message));
    let _ = bar.SetIsOpen(true);
}

/// Creates a thin status `ProgressBar`.
///
/// # Errors
///
/// Returns an error if `ProgressBar` creation fails.
pub fn progress_bar() -> windows_core::Result<ProgressBar> {
    let bar = ProgressBar::new()?;
    bar.SetMinimum(0.0)?;
    bar.SetMaximum(100.0)?;
    bar.SetValue2(0.0)?;
    bar.SetHeight(4.0)?;
    Ok(bar)
}

/// Configures a `ScrollViewer` with optional horizontal and vertical scrollbars.
///
/// # Errors
///
/// Returns an error if `ScrollViewer` creation fails.
pub fn scroll_viewer(auto_scrollbars: bool) -> windows_core::Result<ScrollViewer> {
    let sv = ScrollViewer::new()?;
    let (mode, vis) = if auto_scrollbars {
        (ScrollMode::Auto, ScrollBarVisibility::Auto)
    } else {
        (ScrollMode::Disabled, ScrollBarVisibility::Disabled)
    };
    sv.SetHorizontalScrollMode(mode)?;
    sv.SetVerticalScrollMode(mode)?;
    sv.SetHorizontalScrollBarVisibility(vis)?;
    sv.SetVerticalScrollBarVisibility(vis)?;
    Ok(sv)
}

/// Configures a vertical-only `ScrollViewer` whose content stretches and wraps horizontally.
///
/// # Errors
///
/// Returns an error if `ScrollViewer` creation fails.
pub fn scroll_viewer_vertical() -> windows_core::Result<ScrollViewer> {
    let sv = ScrollViewer::new()?;
    sv.SetHorizontalScrollMode(ScrollMode::Disabled)?;
    sv.SetHorizontalScrollBarVisibility(ScrollBarVisibility::Disabled)?;
    sv.SetVerticalScrollMode(ScrollMode::Auto)?;
    sv.SetVerticalScrollBarVisibility(ScrollBarVisibility::Auto)?;
    Ok(sv)
}

/// Creates a `WriteableBitmap` from premultiplied `BGRA8` pixels (`len == width * height * 4`).
///
/// # Errors
///
/// Returns an error if dimensions are non-positive, buffer length mismatches, or `IBufferByteAccess` fails.
pub fn writeable_bitmap_from_bgra(
    width: i32,
    height: i32,
    bgra: &[u8],
) -> windows_core::Result<WriteableBitmap> {
    if width <= 0 || height <= 0 {
        return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
            -2_147_024_809,
        )));
    }
    let expected = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .and_then(|px| px.checked_mul(4))
        .ok_or_else(|| windows_core::Error::from_hresult(windows_core::HRESULT(-2_147_024_809)))?;
    if bgra.len() != expected {
        return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
            -2_147_024_809,
        )));
    }

    let bmp = WriteableBitmap::CreateInstanceWithDimensions(width, height)?;
    write_bgra_into_bitmap(&bmp, bgra)?;
    Ok(bmp)
}

/// Copies `bgra` pixels directly into an existing matching-size `WriteableBitmap` and invalidates it.
///
/// # Errors
///
/// Returns an error if accessing the underlying `IBufferByteAccess` fails.
pub fn write_bgra_into_bitmap(bmp: &WriteableBitmap, bgra: &[u8]) -> windows_core::Result<()> {
    let buf = bmp.PixelBuffer()?;
    let byte_access: IBufferByteAccess = buf.cast()?;
    // SAFETY: `byte_access` implements `IBufferByteAccess` and returns raw pixel storage pointer.
    let ptr = unsafe { byte_access.Buffer()? };
    if ptr.is_null() {
        return Err(windows_core::Error::from_hresult(windows_core::HRESULT(
            -2_147_467_261,
        )));
    }
    let cap = usize::try_from(buf.Length()?).unwrap_or(0).min(bgra.len());
    // SAFETY: `ptr` points to writable `WriteableBitmap` buffer of at least `buf.Length()` bytes while `buf` lives.
    let dst = unsafe { core::slice::from_raw_parts_mut(ptr, cap) };
    dst.copy_from_slice(&bgra[..cap]);
    bmp.Invalidate()
}
