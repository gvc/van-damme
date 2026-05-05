use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone)]
enum DisplayRow {
    GroupHeader { dir: String, collapsed: bool },
    Separator,
    Item(usize),
}

#[derive(Debug, Clone)]
pub enum VisibleRow<'a, T> {
    GroupHeader { dir: &'a str, collapsed: bool },
    Separator,
    Item { item: &'a T, selected: bool },
}

#[derive(Debug)]
pub struct GroupedList<T> {
    items: Vec<T>,
    display_rows: Vec<DisplayRow>,
    selected: Option<usize>,
    collapsed: HashSet<String>,
}

impl<T> GroupedList<T> {
    pub fn new(items: Vec<T>, group_key: impl Fn(&T) -> &str) -> Self {
        let collapsed = HashSet::new();
        let display_rows = build_display_rows(&items, &collapsed, &group_key);
        let selected = first_selectable(&display_rows);
        Self {
            items,
            display_rows,
            selected,
            collapsed,
        }
    }

    #[allow(dead_code)]
    pub fn items(&self) -> &[T] {
        &self.items
    }

    pub fn items_mut(&mut self) -> &mut [T] {
        &mut self.items
    }

    pub fn replace_items(&mut self, items: Vec<T>, group_key: impl Fn(&T) -> &str) {
        self.items = items;
        self.rebuild(group_key);
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn move_up(&mut self) {
        let selectable = self.selectable_indices();
        if selectable.is_empty() {
            return;
        }
        let next = match self.selected {
            Some(i) => {
                let pos = selectable.iter().position(|&s| s == i).unwrap_or(0);
                if pos == 0 {
                    *selectable.last().unwrap()
                } else {
                    selectable[pos - 1]
                }
            }
            None => selectable[0],
        };
        self.selected = Some(next);
    }

    pub fn move_down(&mut self) {
        let selectable = self.selectable_indices();
        if selectable.is_empty() {
            return;
        }
        let next = match self.selected {
            Some(i) => {
                let pos = selectable.iter().position(|&s| s == i).unwrap_or(0);
                if pos >= selectable.len() - 1 {
                    selectable[0]
                } else {
                    selectable[pos + 1]
                }
            }
            None => selectable[0],
        };
        self.selected = Some(next);
    }

    pub fn selected_item(&self) -> Option<&T> {
        let row_idx = self.selected?;
        match self.display_rows.get(row_idx) {
            Some(DisplayRow::Item(i)) => Some(&self.items[*i]),
            _ => None,
        }
    }

    pub fn selected_header(&self) -> Option<&str> {
        let row_idx = self.selected?;
        match self.display_rows.get(row_idx) {
            Some(DisplayRow::GroupHeader { dir, .. }) if !dir.is_empty() => Some(dir.as_str()),
            _ => None,
        }
    }

    pub fn selected_display_index(&self) -> Option<usize> {
        self.selected
    }

    pub fn select_by(&mut self, pred: impl Fn(&T) -> bool) {
        let item_idx = self.items.iter().position(&pred);
        let display_idx = item_idx.and_then(|si| {
            self.display_rows
                .iter()
                .position(|r| matches!(r, DisplayRow::Item(idx) if *idx == si))
        });
        let fallback = first_selectable(&self.display_rows);
        self.selected = display_idx.or(fallback);
    }

    pub fn toggle_collapse_selected(&mut self, group_key: impl Fn(&T) -> &str) {
        let Some(selected_row) = self.selected else {
            return;
        };

        let group_dir = match self.display_rows.get(selected_row) {
            Some(DisplayRow::GroupHeader { dir, .. }) if !dir.is_empty() => Some(dir.clone()),
            _ => self.display_rows[..=selected_row]
                .iter()
                .rev()
                .find_map(|r| match r {
                    DisplayRow::GroupHeader { dir, .. } if !dir.is_empty() => Some(dir.clone()),
                    _ => None,
                }),
        };

        let Some(dir) = group_dir else { return };

        let was_collapsed = self.collapsed.contains(&dir);
        if was_collapsed {
            self.collapsed.remove(&dir);
        } else {
            self.collapsed.insert(dir.clone());
        }

        self.rebuild(&group_key);

        if was_collapsed {
            let target = self.display_rows.iter().enumerate().find_map(|(i, r)| {
                if let DisplayRow::Item(si) = r
                    && group_key(&self.items[*si]) == dir
                {
                    return Some(i);
                }
                None
            });
            if let Some(i) = target {
                self.selected = Some(i);
                return;
            }
        } else {
            let target = self
                .display_rows
                .iter()
                .position(|r| matches!(r, DisplayRow::GroupHeader { dir: d, .. } if d == &dir));
            if let Some(i) = target {
                self.selected = Some(i);
                return;
            }
        }

        self.clamp_selection();
    }

    pub fn toggle_collapse_all(&mut self, group_key: impl Fn(&T) -> &str) {
        let all_dirs: Vec<String> = {
            let mut seen = BTreeMap::new();
            for item in &self.items {
                seen.insert(group_key(item).to_string(), ());
            }
            seen.into_keys().collect()
        };
        if all_dirs.is_empty() {
            return;
        }
        if all_dirs.iter().all(|d| self.collapsed.contains(d)) {
            self.collapsed.clear();
        } else {
            self.collapsed = all_dirs.into_iter().collect();
        }
        self.rebuild(&group_key);
        self.clamp_selection();
    }

    pub fn visible_rows(&self) -> impl Iterator<Item = VisibleRow<'_, T>> {
        let selected_item_idx =
            self.selected
                .and_then(|row_idx| match self.display_rows.get(row_idx) {
                    Some(DisplayRow::Item(i)) => Some(*i),
                    _ => None,
                });
        let selected_header_row = self.selected;

        self.display_rows
            .iter()
            .enumerate()
            .map(move |(row_idx, row)| match row {
                DisplayRow::GroupHeader { dir, collapsed } => VisibleRow::GroupHeader {
                    dir: dir.as_str(),
                    collapsed: *collapsed,
                },
                DisplayRow::Separator => VisibleRow::Separator,
                DisplayRow::Item(i) => VisibleRow::Item {
                    item: &self.items[*i],
                    selected: selected_item_idx == Some(*i)
                        || (selected_item_idx.is_none() && selected_header_row == Some(row_idx)),
                },
            })
    }

    pub fn is_selected_row(&self, row_display_idx: usize) -> bool {
        self.selected == Some(row_display_idx)
    }

    fn selectable_indices(&self) -> Vec<usize> {
        self.display_rows
            .iter()
            .enumerate()
            .filter_map(|(i, r)| match r {
                DisplayRow::Item(_) => Some(i),
                DisplayRow::GroupHeader {
                    dir,
                    collapsed: true,
                } if !dir.is_empty() => Some(i),
                _ => None,
            })
            .collect()
    }

    fn clamp_selection(&mut self) {
        let selectable = self.selectable_indices();
        if selectable.is_empty() {
            self.selected = None;
            return;
        }
        if self.selected.is_some_and(|cur| selectable.contains(&cur)) {
            return;
        }
        let first_item = selectable
            .iter()
            .copied()
            .find(|&i| matches!(self.display_rows[i], DisplayRow::Item(_)));
        self.selected = Some(first_item.unwrap_or(selectable[0]));
    }

    fn rebuild(&mut self, group_key: impl Fn(&T) -> &str) {
        self.display_rows = build_display_rows(&self.items, &self.collapsed, group_key);
        self.clamp_selection();
    }
}

fn build_display_rows<T>(
    items: &[T],
    collapsed: &HashSet<String>,
    group_key: impl Fn(&T) -> &str,
) -> Vec<DisplayRow> {
    if items.is_empty() {
        return vec![];
    }

    let mut groups: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (i, item) in items.iter().enumerate() {
        groups.entry(group_key(item)).or_default().push(i);
    }

    let mut rows = Vec::new();
    for (dir, indices) in &groups {
        if !rows.is_empty() {
            rows.push(DisplayRow::Separator);
        }
        let is_collapsed = collapsed.contains(*dir);
        rows.push(DisplayRow::GroupHeader {
            dir: dir.to_string(),
            collapsed: is_collapsed,
        });
        if !is_collapsed {
            for &idx in indices {
                rows.push(DisplayRow::Item(idx));
            }
        }
    }
    rows
}

fn first_selectable(rows: &[DisplayRow]) -> Option<usize> {
    rows.iter().position(|r| matches!(r, DisplayRow::Item(_)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq)]
    struct Entry {
        name: String,
        group: String,
    }

    fn entry(name: &str, group: &str) -> Entry {
        Entry {
            name: name.to_string(),
            group: group.to_string(),
        }
    }

    fn gk(e: &Entry) -> &str {
        &e.group
    }

    fn sample_two_groups() -> Vec<Entry> {
        vec![
            entry("alpha", "/proj/a"),
            entry("beta", "/proj/a"),
            entry("gamma", "/proj/b"),
        ]
    }

    fn sample_three_groups() -> Vec<Entry> {
        vec![
            entry("one", "/tmp/one"),
            entry("two", "/tmp/two"),
            entry("three", "/tmp/three"),
        ]
    }

    // --- Construction ---

    #[test]
    fn new_selects_first_item() {
        let list = GroupedList::new(sample_two_groups(), gk);
        assert_eq!(list.selected_item().unwrap().name, "alpha");
    }

    #[test]
    fn new_empty_selects_none() {
        let list: GroupedList<Entry> = GroupedList::new(vec![], gk);
        assert_eq!(list.selected_item(), None);
        assert_eq!(list.selected_display_index(), None);
    }

    // --- Display rows structure ---

    #[test]
    fn display_rows_two_groups() {
        let list = GroupedList::new(sample_two_groups(), gk);
        let rows: Vec<_> = list.visible_rows().collect();
        assert_eq!(rows.len(), 6);
        assert!(matches!(
            rows[0],
            VisibleRow::GroupHeader {
                dir: "/proj/a",
                collapsed: false
            }
        ));
        assert!(matches!(rows[1], VisibleRow::Item { item, .. } if item.name == "alpha"));
        assert!(matches!(rows[2], VisibleRow::Item { item, .. } if item.name == "beta"));
        assert!(matches!(rows[3], VisibleRow::Separator));
        assert!(matches!(
            rows[4],
            VisibleRow::GroupHeader {
                dir: "/proj/b",
                collapsed: false
            }
        ));
        assert!(matches!(rows[5], VisibleRow::Item { item, .. } if item.name == "gamma"));
    }

    #[test]
    fn single_group_no_separator() {
        let items = vec![entry("a", "/same"), entry("b", "/same")];
        let list = GroupedList::new(items, gk);
        let rows: Vec<_> = list.visible_rows().collect();
        assert_eq!(rows.len(), 3);
        assert!(matches!(
            rows[0],
            VisibleRow::GroupHeader { dir: "/same", .. }
        ));
        assert!(matches!(rows[1], VisibleRow::Item { item, .. } if item.name == "a"));
        assert!(matches!(rows[2], VisibleRow::Item { item, .. } if item.name == "b"));
    }

    // --- Navigation ---

    #[test]
    fn move_down_skips_headers() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        assert_eq!(list.selected_item().unwrap().name, "alpha");
        list.move_down();
        assert_eq!(list.selected_item().unwrap().name, "beta");
        list.move_down();
        assert_eq!(list.selected_item().unwrap().name, "gamma");
    }

    #[test]
    fn move_down_wraps() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.move_down(); // beta
        list.move_down(); // gamma
        list.move_down(); // wraps to alpha
        assert_eq!(list.selected_item().unwrap().name, "alpha");
    }

    #[test]
    fn move_up_skips_headers() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        // select gamma
        list.select_by(|e| e.name == "gamma");
        list.move_up();
        assert_eq!(list.selected_item().unwrap().name, "beta");
    }

    #[test]
    fn move_up_wraps() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        // at alpha, move up wraps to gamma
        list.move_up();
        assert_eq!(list.selected_item().unwrap().name, "gamma");
    }

    #[test]
    fn navigation_on_empty_is_noop() {
        let mut list: GroupedList<Entry> = GroupedList::new(vec![], gk);
        list.move_down();
        assert_eq!(list.selected_item(), None);
        list.move_up();
        assert_eq!(list.selected_item(), None);
    }

    // --- select_by ---

    #[test]
    fn select_by_finds_item() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.select_by(|e| e.name == "gamma");
        assert_eq!(list.selected_item().unwrap().name, "gamma");
    }

    #[test]
    fn select_by_falls_back_to_first() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.select_by(|e| e.name == "nonexistent");
        assert_eq!(list.selected_item().unwrap().name, "alpha");
    }

    // --- Collapse ---

    #[test]
    fn collapse_hides_items() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        // at alpha, collapse /proj/a
        list.toggle_collapse_selected(gk);
        let rows: Vec<_> = list.visible_rows().collect();
        assert_eq!(rows.len(), 4);
        assert!(matches!(
            rows[0],
            VisibleRow::GroupHeader {
                dir: "/proj/a",
                collapsed: true
            }
        ));
        assert!(matches!(rows[1], VisibleRow::Separator));
        assert!(matches!(
            rows[2],
            VisibleRow::GroupHeader {
                dir: "/proj/b",
                collapsed: false
            }
        ));
        assert!(matches!(rows[3], VisibleRow::Item { item, .. } if item.name == "gamma"));
    }

    #[test]
    fn collapse_moves_selection_to_header() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.toggle_collapse_selected(gk);
        assert_eq!(list.selected_header(), Some("/proj/a"));
        assert_eq!(list.selected_item(), None);
    }

    #[test]
    fn expand_moves_selection_to_first_item() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.toggle_collapse_selected(gk); // collapse
        list.toggle_collapse_selected(gk); // expand
        assert_eq!(list.selected_item().unwrap().name, "alpha");
    }

    #[test]
    fn collapse_from_non_first_item_finds_group() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.move_down(); // beta
        list.toggle_collapse_selected(gk);
        assert_eq!(list.selected_header(), Some("/proj/a"));
    }

    #[test]
    fn collapse_on_empty_is_noop() {
        let mut list: GroupedList<Entry> = GroupedList::new(vec![], gk);
        list.toggle_collapse_selected(gk);
        assert!(list.collapsed.is_empty());
    }

    #[test]
    fn all_collapsed_header_selected() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.toggle_collapse_selected(gk); // collapse /proj/a, cursor on header
        list.move_down(); // gamma
        list.toggle_collapse_selected(gk); // collapse /proj/b
        assert!(list.selected_header().is_some());
    }

    #[test]
    fn expand_from_header_when_all_collapsed() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.toggle_collapse_selected(gk);
        list.toggle_collapse_selected(gk); // re-expand /proj/a
        let has_item = list
            .visible_rows()
            .any(|r| matches!(r, VisibleRow::Item { .. }));
        assert!(has_item);
    }

    // --- toggle_collapse_all ---

    #[test]
    fn collapse_all() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.toggle_collapse_all(gk);
        assert!(list.collapsed.contains("/proj/a"));
        assert!(list.collapsed.contains("/proj/b"));
        let has_item = list
            .visible_rows()
            .any(|r| matches!(r, VisibleRow::Item { .. }));
        assert!(!has_item);
    }

    #[test]
    fn expand_all_when_all_collapsed() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.toggle_collapse_all(gk);
        list.toggle_collapse_all(gk);
        assert!(list.collapsed.is_empty());
        let has_item = list
            .visible_rows()
            .any(|r| matches!(r, VisibleRow::Item { .. }));
        assert!(has_item);
    }

    #[test]
    fn collapse_all_when_partially_collapsed() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.toggle_collapse_selected(gk); // collapse /proj/a only
        list.toggle_collapse_all(gk); // should collapse all
        assert_eq!(list.collapsed.len(), 2);
    }

    #[test]
    fn collapse_all_on_empty_is_noop() {
        let mut list: GroupedList<Entry> = GroupedList::new(vec![], gk);
        list.toggle_collapse_all(gk);
        assert!(list.collapsed.is_empty());
    }

    // --- Mixed navigation ---

    #[test]
    fn navigate_mixed_collapsed_expanded() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.toggle_collapse_selected(gk); // collapse /proj/a
        assert_eq!(list.selected_header(), Some("/proj/a"));
        list.move_down(); // gamma
        assert_eq!(list.selected_item().unwrap().name, "gamma");
        list.move_up(); // back to collapsed header
        assert_eq!(list.selected_header(), Some("/proj/a"));
    }

    #[test]
    fn navigate_between_collapsed_headers() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.toggle_collapse_selected(gk); // collapse /proj/a
        list.move_down(); // gamma
        list.toggle_collapse_selected(gk); // collapse /proj/b
        let before = list.selected_display_index();
        list.move_down();
        let after = list.selected_display_index();
        assert_ne!(before, after);
    }

    // --- replace_items ---

    #[test]
    fn replace_items_rebuilds() {
        let mut list = GroupedList::new(sample_two_groups(), gk);
        list.move_down(); // beta
        list.replace_items(vec![entry("new", "/new")], gk);
        assert_eq!(list.selected_item().unwrap().name, "new");
    }

    // --- is_selected_row ---

    #[test]
    fn is_selected_row_works() {
        let list = GroupedList::new(sample_two_groups(), gk);
        let sel = list.selected_display_index().unwrap();
        assert!(list.is_selected_row(sel));
        assert!(!list.is_selected_row(sel + 1));
    }
}
