use super::*;

impl Document {
    /// Change explicit locks without changing descendants' own lock flags.
    /// A child stays protected until all its locked ancestors are unlocked.
    pub fn set_nodes_locked(&mut self, selected: &HashSet<usize>, locked: bool) -> Result<bool> {
        if selected.iter().any(|&index| index >= self.nodes.len()) {
            return Err("所选节点不存在".into());
        }
        if selected
            .iter()
            .all(|&index| self.nodes[index].locked == locked)
        {
            return Ok(false);
        }
        self.finish();
        self.begin(if locked {
            "锁定节点"
        } else {
            "解锁节点"
        });
        for &index in selected {
            self.nodes[index].locked = locked;
        }
        Ok(self.finish())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locking_nodes_is_undoable_and_persists_on_reopen() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        let selected = HashSet::from([0]);
        assert!(doc.set_nodes_locked(&selected, true).unwrap());
        assert!(doc.nodes[0].locked);
        assert!(doc.expand_selection(&selected).is_empty());
        doc.delete_objects(&selected, &HashSet::new()).unwrap();
        assert_eq!(doc.nodes.len(), 1);
        assert!(!doc.set_nodes_locked(&selected, true).unwrap());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("locked.prg");
        doc.save(&path).unwrap();
        assert!(Document::open(&path).unwrap().nodes[0].locked);
        doc.undo();
        assert!(!doc.nodes[0].locked);
        doc.redo();
        assert!(doc.nodes[0].locked);
        assert!(doc.set_nodes_locked(&selected, false).unwrap());
        assert_eq!(doc.expand_selection(&selected), selected);
        doc.undo();
        assert!(doc.nodes[0].locked);
    }

    #[test]
    fn locked_groups_protect_directly_selected_members_and_keep_child_locks() {
        let mut doc = Document::blank().unwrap();
        doc.create_text_node([0.0; 2]).unwrap();
        doc.create_text_node([200.0, 0.0]).unwrap();
        let group = doc.create_group(&HashSet::from([0, 1])).unwrap();
        doc.set_nodes_locked(&HashSet::from([0]), true).unwrap();
        doc.set_nodes_locked(&HashSet::from([group]), true).unwrap();
        assert!(doc.expand_selection(&HashSet::from([1])).is_empty());
        assert!(!doc.can_edit_text(1));
        doc.delete_objects(&HashSet::from([1]), &HashSet::new())
            .unwrap();
        assert_eq!(doc.nodes.len(), 3);
        doc.set_nodes_locked(&HashSet::from([group]), false)
            .unwrap();
        assert!(doc.nodes[0].locked);
        assert!(doc.can_edit_text(1));
        assert_eq!(
            doc.expand_selection(&HashSet::from([1])),
            HashSet::from([1])
        );
    }
}
