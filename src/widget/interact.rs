use super::{ChemStructEditor, DEFAULT_BOND_LENGTH};
use eframe::egui;

pub fn process_bond_tool(
    editor: &mut ChemStructEditor,
    response: &egui::Response,
    center: egui::Pos2,
    ui: &egui::Ui,
) -> bool {
    let mut modified = false;

    let mouse = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);

    // Track drag origin on press
    if response.drag_started() {
        editor.drag_origin_screen = Some(mouse);
        let hit = editor.hit_test_atom(mouse, center);
        if hit.is_some() {
            // Start drag: could be atom move or bond draw
            // We'll decide on first significant movement
            editor.dragging_atom = hit;
            editor.bond_start = None;
        } else {
            editor.dragging_atom = None;
            editor.bond_start = None;
        }
    }

    if response.dragged() {
        let drag_delta = response.drag_delta();
        let total_delta = mouse
            - editor
                .drag_origin_screen
                .unwrap_or(mouse - drag_delta);

        if let Some(atom_id) = editor.dragging_atom {
            // Move atom
            let inv = 1.0 / (editor.zoom * super::SCALE_FACTOR);
            let dx = drag_delta.x * inv;
            let dy = drag_delta.y * inv;
            if let Some(atom) = editor.molecule.atom_by_id_mut(atom_id) {
                atom.pos[0] += dx;
                atom.pos[1] += dy;
                modified = true;
            }
            editor.bond_start = None;
            editor.preview_end_screen = None;
        } else if total_delta.length() > 4.0 {
            // Bond drawing from empty space: snap to nearest atom as source
            if editor.bond_start.is_none() {
                let origin = editor.drag_origin_screen.unwrap_or(mouse);
                editor.bond_start = editor.hit_test_atom(origin, center);
            }
            editor.preview_end_screen = Some(mouse);
        }
    }

    if response.drag_stopped() {
        let start_id = editor.bond_start.take();
        let end_screen = editor.preview_end_screen.take();

        if let (Some(src_id), Some(end)) = (start_id, end_screen) {
            let snap_target = editor.hit_test_atom(end, center);
            let target_id = if let Some(id) = snap_target {
                id
            } else {
                let mol_pos = editor.screen_to_mol(end, center);
                editor
                    .molecule
                    .add_atom(editor.current_element.clone(), mol_pos, 0)
            };

            if target_id != src_id {
                if let Some(existing_bond) = editor
                    .molecule
                    .bond_between(src_id, target_id)
                    .map(|b| b.id)
                {
                    // Cycle existing bond order instead of adding duplicate
                    if let Some(bond) = editor.molecule.bond_by_id_mut(existing_bond) {
                        bond.order = bond.order.cycle();
                        modified = true;
                    }
                } else {
                    editor.molecule.add_bond(
                        src_id,
                        target_id,
                        editor.current_bond_order.clone(),
                    );
                    modified = true;
                }
            }
        }

        editor.dragging_atom = None;
        editor.drag_start_mol = None;
        editor.drag_origin_screen = None;
    }

    // Click (no significant drag)
    if response.clicked() {
        if let Some(atom_id) = editor.hovered_atom {
            let angle = editor.best_new_bond_angle(atom_id);
            let new_pos = {
                let src = editor.molecule.atom_by_id(atom_id).unwrap();
                [
                    src.pos[0] + angle.cos() * DEFAULT_BOND_LENGTH,
                    src.pos[1] + angle.sin() * DEFAULT_BOND_LENGTH,
                ]
            };
            let new_atom_id = find_or_create_atom(editor, new_pos, center);
            if new_atom_id != atom_id {
                if let Some(existing_bond) =
                    editor.molecule.bond_between(atom_id, new_atom_id).map(|b| b.id)
                {
                    if let Some(bond) = editor.molecule.bond_by_id_mut(existing_bond) {
                        bond.order = bond.order.cycle();
                        modified = true;
                    }
                } else {
                    editor.molecule.add_bond(
                        atom_id,
                        new_atom_id,
                        editor.current_bond_order.clone(),
                    );
                    modified = true;
                }
            }
        } else if let Some(bond_id) = editor.hovered_bond {
            if let Some(bond) = editor.molecule.bond_by_id_mut(bond_id) {
                bond.order = bond.order.cycle();
                modified = true;
            }
        } else {
            let mol_pos = editor.screen_to_mol(mouse, center);
            editor
                .molecule
                .add_atom(editor.current_element.clone(), mol_pos, 0);
            modified = true;
        }
    }

    // Middle-click or right-click drag for pan
    if ui.input(|i| i.pointer.middle_down()) {
        if let Some(delta) = ui.input(|i| i.pointer.delta()).into() {
            editor.pan += delta;
        }
    }

    modified
}

pub fn process_select_tool(
    editor: &mut ChemStructEditor,
    response: &egui::Response,
    center: egui::Pos2,
    ui: &egui::Ui,
) -> bool {
    let mut modified = false;
    let mouse = response.interact_pointer_pos().unwrap_or(egui::Pos2::ZERO);

    if response.clicked() {
        editor.lasso_path.clear();
        if let Some(atom_id) = editor.hovered_atom {
            if editor.selected_atoms.contains(&atom_id) {
                editor.selected_atoms.remove(&atom_id);
            } else {
                editor.selected_atoms.insert(atom_id);
            }
        } else {
            editor.selected_atoms.clear();
        }
    }

    if response.drag_started() {
        editor.lasso_path.clear();
        editor.drag_origin_screen = Some(mouse);
        editor.dragging_atom = editor.hovered_atom;
    }

    if response.dragged() {
        if let Some(atom_id) = editor.dragging_atom {
            let drag_delta = response.drag_delta();
            let inv = 1.0 / (editor.zoom * super::SCALE_FACTOR);
            let dx = drag_delta.x * inv;
            let dy = drag_delta.y * inv;

            let atoms_to_move: Vec<u32> = if editor.selected_atoms.contains(&atom_id) {
                editor.selected_atoms.iter().cloned().collect()
            } else {
                vec![atom_id]
            };
            for id in atoms_to_move {
                if let Some(atom) = editor.molecule.atom_by_id_mut(id) {
                    atom.pos[0] += dx;
                    atom.pos[1] += dy;
                }
            }
            modified = true;
        } else {
            editor.lasso_path.push(mouse);
        }
    }

    if response.drag_stopped() {
        if editor.dragging_atom.is_some() {
            editor.dragging_atom = None;
        } else if editor.lasso_path.len() > 2 {
            editor.selected_atoms.clear();
            let lasso = editor.lasso_path.clone();
            let hits: Vec<u32> = editor
                .molecule
                .atoms
                .iter()
                .filter_map(|atom| {
                    let sp = editor.mol_to_screen(atom.pos, center);
                    if is_point_in_polygon(sp, &lasso) {
                        Some(atom.id)
                    } else {
                        None
                    }
                })
                .collect();
            editor.selected_atoms.extend(hits);
        }
        editor.lasso_path.clear();
        editor.drag_origin_screen = None;
    }

    // Delete/Backspace removes selected atoms
    let delete_pressed =
        ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace));
    if delete_pressed && !editor.selected_atoms.is_empty() {
        let ids: Vec<u32> = editor.selected_atoms.drain().collect();
        for id in ids {
            editor.molecule.remove_atom(id);
        }
        modified = true;
    }

    modified
}

pub fn process_eraser_tool(
    editor: &mut ChemStructEditor,
    response: &egui::Response,
    _center: egui::Pos2,
) -> bool {
    let mut modified = false;
    if response.clicked() {
        if let Some(atom_id) = editor.hovered_atom {
            editor.molecule.remove_atom(atom_id);
            modified = true;
        } else if let Some(bond_id) = editor.hovered_bond {
            editor.molecule.remove_bond(bond_id);
            modified = true;
        }
    }
    modified
}

fn find_or_create_atom(
    editor: &mut ChemStructEditor,
    mol_pos: [f32; 2],
    center: egui::Pos2,
) -> u32 {
    let screen = editor.mol_to_screen(mol_pos, center);
    if let Some(id) = editor.hit_test_atom(screen, center) {
        return id;
    }
    editor
        .molecule
        .add_atom(editor.current_element.clone(), mol_pos, 0)
}

fn is_point_in_polygon(point: egui::Pos2, polygon: &[egui::Pos2]) -> bool {
    if polygon.len() < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = polygon.len() - 1;
    for i in 0..polygon.len() {
        let pi = polygon[i];
        let pj = polygon[j];
        if ((pi.y > point.y) != (pj.y > point.y))
            && (point.x < (pj.x - pi.x) * (point.y - pi.y) / (pj.y - pi.y) + pi.x)
        {
            inside = !inside;
        }
        j = i;
    }
    inside
}
