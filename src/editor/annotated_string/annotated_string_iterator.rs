use super::{AnnotatedString, AnnotatedStringPart, AnnotationType};
use std::cmp::min;

pub struct AnnotatedStringIterator<'a> {
    pub annotated_string: &'a AnnotatedString,
    pub current_idx: usize,
}

impl<'a> Iterator for AnnotatedStringIterator<'a> {
    type Item = AnnotatedStringPart<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.current_idx >= self.annotated_string.string.len() {
            return None;
        }

        let s = &self.annotated_string.string;
        // Ensure we never slice from the middle of a UTF-8 character.
        while self.current_idx > 0 && !s.is_char_boundary(self.current_idx) {
            self.current_idx -= 1;
        }

        let covering: Vec<_> = self
            .annotated_string
            .annotations
            .iter()
            .filter(|a| a.start <= self.current_idx && a.end > self.current_idx)
            .collect();

        let annotation = covering
            .iter()
            .find(|a| a.kind == AnnotationType::Selection)
            .copied()
            .or_else(|| covering.last().copied());
        if let Some(annotation) = annotation {
            let start_idx = self.current_idx;
            let mut end_idx = min(annotation.end, self.annotated_string.string.len());
            // Don't skip over the start of another annotation: split at boundaries
            // so that e.g. Selection (2..5) inside Comment (0..74) gets emitted.
            for a in &self.annotated_string.annotations {
                if a.start > start_idx && a.start < end_idx {
                    end_idx = a.start;
                }
            }
            // Ensure we always slice on char boundaries to avoid UTF-8 panics.
            let s = &self.annotated_string.string;
            while end_idx > start_idx && !s.is_char_boundary(end_idx) {
                end_idx -= 1;
            }
            if end_idx <= start_idx {
                // Fallback: advance by one byte and try again.
                self.current_idx = start_idx + 1;
                return self.next();
            }
            self.current_idx = end_idx;
            return Some(AnnotatedStringPart {
                string: &self.annotated_string.string[start_idx..end_idx],
                annotation_type: Some(annotation.kind),
            });
        }
        let mut end_idx = self.annotated_string.string.len();
        for annotation in &self.annotated_string.annotations {
            if annotation.start > self.current_idx && annotation.start < end_idx {
                end_idx = annotation.start;
            }
        }
        let start_idx = self.current_idx;
        let s = &self.annotated_string.string;
        while end_idx > start_idx && !s.is_char_boundary(end_idx) {
            end_idx -= 1;
        }
        if end_idx <= start_idx {
            self.current_idx = start_idx + 1;
            return self.next();
        }
        self.current_idx = end_idx;

        Some(AnnotatedStringPart {
            string: &self.annotated_string.string[start_idx..end_idx],
            annotation_type: None,
        })
    }
}
