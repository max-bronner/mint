use super::*;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn columns(percents: &[f32]) -> Vec<ListColumn> {
    percents
        .iter()
        .map(|&percent| ListColumn {
            percent,
            align: Align::Left,
        })
        .collect()
}

#[test]
fn the_frame_is_eas_scroll_bar_and_columns() {
    let frame = ListFrame::new(
        [0.0, 0.0, 300.0, 150.0],
        true,
        &columns(&[10.0, 70.0, 20.0]),
    );
    assert_eq!(frame.up, Some([277.0, 2.0, 298.0, 24.0]));
    assert_eq!(frame.down, Some([277.0, 126.0, 298.0, 148.0]));
    assert_eq!(frame.track, Some([277.0, 25.0, 298.0, 125.0]));
    assert_eq!(frame.rows, [0.0, 4.0, 276.0, 150.0]);
    assert_eq!(frame.clip, [1.0, 1.0, 275.0, 149.0]);
    let widths: Vec<f32> = frame.columns.iter().map(|[a, b]| b - a).collect();
    assert!(close(widths[0], 27.9) && close(widths[1], 195.3) && close(widths[2], 55.8));
    assert!(
        close(frame.columns[2][1], 279.0),
        "percent of the width less the bar"
    );

    let one = ListFrame::new([10.0, 0.0, 110.0, 50.0], true, &[]);
    assert_eq!(one.columns, vec![[10.0, 89.0]]);
    let bare = ListFrame::new([0.0, 0.0, 100.0, 50.0], false, &[]);
    assert_eq!((bare.up, bare.track), (None, None));
    assert_eq!(bare.rows[2], 100.0);
}

#[test]
fn scrolling_steps_a_row_and_stops_at_the_ends() {
    let heights = [10.0; 10];
    assert!(close(stepped(&heights, 0.0, 50.0, 1), 11.0));
    assert!(close(stepped(&heights, 11.0, 50.0, -1), 0.0));
    assert!(close(stepped(&heights, 0.0, 50.0, -1), 0.0));
    assert!(close(stepped(&heights, 22.0, 50.0, 2), 44.0));
    assert!(
        close(stepped(&heights, 66.0, 50.0, 1), 66.0),
        "the end is in view"
    );
    assert!(close(stepped(&[], 0.0, 50.0, 1), 0.0));
    assert!(
        close(max_display(&heights, 50.0), 63.0),
        "110 - (50 - 2) + 1"
    );
    assert!(close(max_display(&heights, 500.0), 0.0));
    assert_eq!(top_entry(&heights, 12.0), 1);
}

#[test]
fn a_selection_is_brought_into_view_as_ea_populates() {
    let heights = [10.0; 20];
    assert!(
        close(revealed(&heights, 0.0, 50.0, 10), 88.0),
        "row 8 on top"
    );
    assert!(close(revealed(&heights, 88.0, 50.0, 2), 22.0));
    assert!(close(revealed(&heights, 0.0, 50.0, 3), 0.0));
    assert!(
        close(revealed(&heights, 0.0, 50.0, 19), 173.0),
        "no further than the range"
    );
}

#[test]
fn the_thumb_travels_the_track() {
    let track = [0.0, 25.0, 21.0, 125.0];
    assert_eq!(thumb_rect(track, 0.0, 63.0), [0.0, 25.0, 21.0, 41.0]);
    assert_eq!(thumb_rect(track, 63.0, 63.0), [0.0, 109.0, 21.0, 125.0]);
    assert!(close(thumb_rect(track, 31.5, 63.0)[1], 67.0));
    assert_eq!(thumb_rect(track, 5.0, 0.0)[1], 25.0);
}

#[test]
fn a_thumb_drag_maps_the_pointer_onto_the_track() {
    let track = [0.0, 25.0, 21.0, 125.0];
    assert!(
        close(thumb_drag(track, 0.0, 25.0, 63.0), 0.0),
        "grabbed at top"
    );
    assert!(
        close(thumb_drag(track, 0.0, 109.0, 63.0), 63.0),
        "dragged to the bottom"
    );
    assert!(
        close(thumb_drag(track, 8.0, 67.0, 63.0), 25.5),
        "mid-thumb grab"
    );
    assert_eq!(thumb_drag(track, 0.0, 10.0, 63.0), 0.0, "above the track");
    assert_eq!(thumb_drag(track, 0.0, 999.0, 63.0), 63.0, "below the track");
}

#[test]
fn starting_and_dragging_the_thumb_scrolls_without_selecting() {
    let frame = ListFrame::new([0.0, 0.0, 300.0, 150.0], true, &[]);
    let mut view = ListView::new(text_rows(30));
    view.laid_out(frame, vec![10.0; 30]);
    assert_eq!(view.part_at([285.0, 30.0]), Some(ListPart::Thumb));
    assert!(view.start_thumb_drag([285.0, 30.0]));
    let max = max_display(&view.heights, view.frame.as_ref().unwrap().display_height());
    view.drag_thumb([285.0, 999.0]);
    assert!(close(view.display_pos, max), "dragged to the end");
    assert_eq!(view.selected, None, "a drag never selects");
    view.end_thumb_drag();
    assert_eq!(view.thumb_grab, None);
}

#[test]
fn the_bar_row_follows_the_hover_while_tracking() {
    assert_eq!(bar_row(true, Some(2), Some(0)), Some(2));
    assert_eq!(bar_row(false, Some(2), Some(0)), Some(0));
    assert_eq!(bar_row(true, None, Some(0)), None);
}

#[test]
fn a_point_names_the_part_under_it() {
    let frame = ListFrame::new([0.0, 0.0, 300.0, 150.0], true, &[]);
    let heights = [10.0; 20];
    let at = |p: [f32; 2], pos: f32| hit(&frame, &heights, pos, p);
    assert_eq!(at([280.0, 10.0], 0.0), Some(ListPart::Up));
    assert_eq!(at([280.0, 140.0], 0.0), Some(ListPart::Down));
    assert_eq!(at([280.0, 30.0], 0.0), Some(ListPart::Thumb));
    assert_eq!(at([280.0, 75.0], 0.0), Some(ListPart::Track(0.5)));
    assert_eq!(at([50.0, 4.0 + 22.0 + 5.0], 0.0), Some(ListPart::Row(2)));
    assert_eq!(at([50.0, 4.0 + 22.0 + 5.0], 11.0), Some(ListPart::Row(3)));
    assert_eq!(at([276.5, 50.0], 0.0), Some(ListPart::Body));
    assert_eq!(at([301.0, 50.0], 0.0), None);
}

#[test]
fn text_wraps_at_spaces_and_a_long_word_stands_alone() {
    #[allow(clippy::cast_precision_loss)]
    let measure = |s: &str| s.len() as f32;
    assert_eq!(wrapped("aa bb cc", 5.0, measure), vec!["aa bb", "cc"]);
    assert_eq!(wrapped("abcdefgh ij", 5.0, measure), vec!["abcdefgh", "ij"]);
    assert_eq!(wrapped("a\nb", 9.0, measure), vec!["a", "b"]);
    assert_eq!(wrapped("", 9.0, measure), vec![""]);
}

const TEMPLATE: &str = "WINDOW
 WINDOWTYPE = SCROLLLISTBOX;
 FONT = NAME: \"Face\", SIZE: 12, BOLD: 0;
 TEXTCOLOR = ENABLED: 255 0 0 255, ENABLEDBORDER: 0 0 0 255;
 ENABLEDDRAWDATA = IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: NoImage, COLOR: 0 255 0 255, BORDERCOLOR: 0 0 255 255,
   IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: Sel, COLOR: 1 1 1 1, BORDERCOLOR: 1 1 1 1;
 HILITEDRAWDATA = IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: SelLit, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 LISTBOXDATA = SCROLLBAR: 0, COLUMNS: 1;
 LISTBOXENABLEDUPBUTTONDRAWDATA = IMAGE: Up, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 LISTBOXHILITEUPBUTTONDRAWDATA = IMAGE: UpLit, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
 LISTBOXENABLEDSLIDERDRAWDATA = IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 51 102 153 255;
 SLIDERTHUMBENABLEDDRAWDATA = IMAGE: Top, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: TopLit, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: NoImage, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: MidLit, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: BotLit, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: Mid, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0,
   IMAGE: Bot, COLOR: 0 0 0 0, BORDERCOLOR: 0 0 0 0;
END
";

#[test]
fn the_look_reads_each_part_from_its_slot() {
    let layout = Layout::parse(TEMPLATE).unwrap();
    let look = ListLook::read(&layout);
    assert_eq!((look.font.as_str(), look.points), ("Face", 12.0));
    assert_eq!(look.text, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(look.selected[0].image.as_deref(), Some("Sel"));
    assert_eq!(look.selected[0].fill, [0.0, 1.0, 0.0, 1.0]);
    assert_eq!(look.selected[0].border, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(look.selected[1].image.as_deref(), Some("SelLit"));
    assert_eq!(look.up[0].as_deref(), Some("Up"));
    assert_eq!(look.up[1].as_deref(), Some("UpLit"));
    assert_eq!(look.down[0], None);
    assert_eq!(look.track.border, [0.2, 0.4, 0.6, 1.0]);
    let thumb = &look.thumb[0];
    assert_eq!(
        (
            thumb.top.as_deref(),
            thumb.middle.as_deref(),
            thumb.bottom.as_deref()
        ),
        (Some("Top"), Some("Mid"), Some("Bot"))
    );
    assert!(!look.scroll_bar);
}

#[test]
fn resolving_a_look_sizes_the_thumb_caps() {
    let layout = Layout::parse(TEMPLATE).unwrap();
    let resolved = ListLook::read(&layout).resolved(|name: &String| match name.as_str() {
        "Top" => Some((1u8, [20.0, 5.0])),
        "Bot" => Some((2u8, [20.0, 10.0])),
        "Sel" => Some((3u8, [8.0, 8.0])),
        _ => None,
    });
    assert_eq!(resolved.thumb[0].caps, [0.25, 0.5]);
    assert_eq!(resolved.thumb[0].middle, None);
    assert_eq!(resolved.selected[0].image, Some(3));
    assert_eq!(resolved.up[0], None);
}

fn text_rows(n: usize) -> Arc<ListContent> {
    Arc::new(ListContent {
        rows: (0..n)
            .map(|i| vec![ListCell::Text(format!("row {i}"))])
            .collect(),
        ..ListContent::default()
    })
}

#[test]
fn a_list_view_selects_scrolls_and_follows_its_host() {
    let frame = ListFrame::new([0.0, 0.0, 300.0, 50.0], true, &[]);
    let mut view = ListView::new(text_rows(20));
    view.laid_out(frame.clone(), vec![10.0; 20]);
    assert_eq!(view.click([50.0, 4.0 + 11.0 + 2.0]), Some(1));
    assert_eq!(view.selected, Some(1));
    assert_eq!(
        view.click([50.0, 4.0 + 11.0 + 2.0]),
        None,
        "already selected"
    );
    view.click([280.0, 40.0]);
    assert!(close(view.display_pos, 11.0), "down a row");
    view.click([280.0, 10.0]);
    assert!(close(view.display_pos, 0.0), "up a row");

    let mut hosted = (*text_rows(20)).clone();
    hosted.selected = Some(15);
    view.set_content(Arc::new(hosted));
    assert_eq!(view.selected, Some(15));
    view.laid_out(frame, vec![10.0; 20]);
    assert!(view.display_pos > 100.0, "brought into view");
    view.scroll(-1);
    assert!(view.display_pos < 150.0);
}

#[test]
fn an_image_cell_is_a_square_no_wider_than_its_column() {
    assert_eq!(image_cell(None, Some([24.0, 26.0]), 27.4), [24.0, 24.0]);
    assert_eq!(image_cell(None, Some([24.0, 26.0]), 20.0), [20.0, 20.0]);
    assert_eq!(image_cell(None, None, 20.0), [20.0, 20.0]);
    assert_eq!(
        image_cell(Some([5.0, 6.0]), Some([24.0, 26.0]), 20.0),
        [5.0, 6.0]
    );
}

#[test]
fn a_row_is_its_tallest_cell_and_text_sits_centred() {
    assert_eq!(row_height([24.0, 16.0, 16.0], 16.0), 24.0);
    assert_eq!(row_height([32.0], 16.0), 32.0);
    assert_eq!(row_height([], 16.0), 16.0);
    assert_eq!(text_top(100.0, 24.0, 16.0), 104.0);
    assert_eq!(text_top(100.0, 16.0, 32.0), 100.0, "never above the row");
}

#[test]
fn a_scrolllistbox_bar_is_there_only_when_the_content_overflows() {
    let layout = Layout::parse(TEMPLATE).unwrap();
    assert!(ListLook::read(&layout).auto_hide);
    let bounds = [0.0, 0.0, 200.0, 100.0];
    let frame = ListFrame::new(bounds, true, &[]);
    let short = frame.clone().hiding_bar_while_content_fits(&[20.0]);
    assert_eq!((short.up, short.down, short.track), (None, None, None));
    assert_eq!(short.columns, frame.columns);
    let tall = frame.clone().hiding_bar_while_content_fits(&[60.0, 60.0]);
    assert_eq!(tall, frame);
    assert!(tall.up.is_some() && tall.track.is_some());
}
