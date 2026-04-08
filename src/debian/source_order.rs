/// Represents the declaration order of a package's source.
/// Lower values indicate earlier declaration (first-declared source wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct SourceOrder {
    source: usize,
    suite: usize,
    component: usize,
}

impl SourceOrder {
    pub(crate) fn new(source: usize, suite: usize, component: usize) -> Self {
        Self {
            source,
            suite,
            component,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ordering_by_source_index() {
        assert!(SourceOrder::new(0, 0, 0) < SourceOrder::new(1, 0, 0));
    }

    #[test]
    fn test_ordering_by_suite_index() {
        assert!(SourceOrder::new(0, 0, 0) < SourceOrder::new(0, 1, 0));
    }

    #[test]
    fn test_ordering_by_component_index() {
        assert!(SourceOrder::new(0, 0, 0) < SourceOrder::new(0, 0, 1));
    }

    #[test]
    fn test_ordering_source_takes_precedence_over_suite() {
        assert!(SourceOrder::new(0, 9, 9) < SourceOrder::new(1, 0, 0));
    }

    #[test]
    fn test_ordering_suite_takes_precedence_over_component() {
        assert!(SourceOrder::new(0, 0, 9) < SourceOrder::new(0, 1, 0));
    }

    #[test]
    fn test_equality() {
        assert_eq!(SourceOrder::new(1, 2, 3), SourceOrder::new(1, 2, 3));
    }
}
