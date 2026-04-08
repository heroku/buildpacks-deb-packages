/// Represents the declaration order of a package's source.
/// Lower values = higher priority (first-declared source wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct PackagePriority {
    source: usize,
    suite: usize,
    component: usize,
}

impl PackagePriority {
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
        assert!(PackagePriority::new(0, 0, 0) < PackagePriority::new(1, 0, 0));
    }

    #[test]
    fn test_ordering_by_suite_index() {
        assert!(PackagePriority::new(0, 0, 0) < PackagePriority::new(0, 1, 0));
    }

    #[test]
    fn test_ordering_by_component_index() {
        assert!(PackagePriority::new(0, 0, 0) < PackagePriority::new(0, 0, 1));
    }

    #[test]
    fn test_ordering_source_takes_precedence_over_suite() {
        assert!(PackagePriority::new(0, 9, 9) < PackagePriority::new(1, 0, 0));
    }

    #[test]
    fn test_ordering_suite_takes_precedence_over_component() {
        assert!(PackagePriority::new(0, 0, 9) < PackagePriority::new(0, 1, 0));
    }

    #[test]
    fn test_equality() {
        assert_eq!(PackagePriority::new(1, 2, 3), PackagePriority::new(1, 2, 3));
    }
}
