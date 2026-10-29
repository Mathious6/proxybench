pub fn evenly<T: Clone>(items: &[T], count: usize) -> Vec<T> {
    if count >= items.len() {
        return items.to_vec();
    }
    (0..count)
        .map(|index| items[index * items.len() / count].clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_evenly_spaced_entries() {
        let items: Vec<u32> = (0..10).collect();
        assert_eq!(evenly(&items, 2), vec![0, 5]);
        assert_eq!(evenly(&items, 3), vec![0, 3, 6]);
        assert_eq!(evenly(&items, 1), vec![0]);
    }

    #[test]
    fn returns_everything_when_count_covers_the_list() {
        let items: Vec<u32> = (0..3).collect();
        assert_eq!(evenly(&items, 3), items);
        assert_eq!(evenly(&items, 10), items);
    }

    #[test]
    fn empty_list_stays_empty() {
        assert!(evenly::<u32>(&[], 2).is_empty());
    }
}
