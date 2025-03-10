#![allow(clippy::too_many_arguments)]
use super::tool_prelude::*;
use crate::consts::{ROTATE_SNAP_ANGLE, SELECTION_TOLERANCE};
use crate::messages::input_mapper::utility_types::input_mouse::ViewportPosition;
use crate::messages::portfolio::document::utility_types::misc::{AlignAggregate, AlignAxis, FlipAxis};
use crate::messages::portfolio::document::utility_types::transformation::Selected;
use crate::messages::tool::common_functionality::graph_modification_utils::is_shape_layer;
use crate::messages::tool::common_functionality::graph_modification_utils::is_text_layer;
use crate::messages::tool::common_functionality::path_outline::*;
use crate::messages::tool::common_functionality::pivot::Pivot;
use crate::messages::tool::common_functionality::snapping::{self, SnapManager};
use crate::messages::tool::common_functionality::transformation_cage::*;
use document_legacy::document::Document;
use document_legacy::document_metadata::LayerNodeIdentifier;
use document_legacy::LayerId;
use document_legacy::Operation;
use graphene_core::renderer::Quad;

use std::fmt;

#[derive(Default)]
pub struct SelectTool {
	fsm_state: SelectToolFsmState,
	tool_data: SelectToolData,
}

#[allow(dead_code)]
#[derive(Default)]
pub struct SelectOptions {
	nested_selection_behavior: NestedSelectionBehavior,
}

#[remain::sorted]
#[derive(PartialEq, Eq, Clone, Debug, Hash, Serialize, Deserialize, specta::Type)]
pub enum SelectOptionsUpdate {
	NestedSelectionBehavior(NestedSelectionBehavior),
}

#[derive(Default, PartialEq, Eq, Clone, Copy, Debug, Hash, Serialize, Deserialize, specta::Type)]
pub enum NestedSelectionBehavior {
	#[default]
	Deepest,
	Shallowest,
}

impl fmt::Display for NestedSelectionBehavior {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			NestedSelectionBehavior::Deepest => write!(f, "Deep Select"),
			NestedSelectionBehavior::Shallowest => write!(f, "Shallow Select"),
		}
	}
}

#[remain::sorted]
#[impl_message(Message, ToolMessage, Select)]
#[derive(PartialEq, Eq, Clone, Debug, Serialize, Deserialize, specta::Type)]
pub enum SelectToolMessage {
	// Standard messages
	#[remain::unsorted]
	Abort,
	#[remain::unsorted]
	DocumentIsDirty,
	#[remain::unsorted]
	SelectionChanged,

	// Tool-specific messages
	DragStart {
		add_to_selection: Key,
		select_deepest: Key,
	},
	DragStop {
		remove_from_selection: Key,
	},
	EditLayer,
	Enter,
	PointerMove {
		axis_align: Key,
		snap_angle: Key,
		center: Key,
		duplicate: Key,
	},
	SelectOptions(SelectOptionsUpdate),
	SetPivot {
		position: PivotPosition,
	},
}

impl ToolMetadata for SelectTool {
	fn icon_name(&self) -> String {
		"GeneralSelectTool".into()
	}
	fn tooltip(&self) -> String {
		"Select Tool".into()
	}
	fn tool_type(&self) -> crate::messages::tool::utility_types::ToolType {
		ToolType::Select
	}
}

impl SelectTool {
	// fn deep_selection_widget(&self) -> WidgetHolder {
	// 	let layer_selection_behavior_entries = [NestedSelectionBehavior::Deepest, NestedSelectionBehavior::Shallowest]
	// 		.iter()
	// 		.map(|mode| {
	// 			MenuListEntry::new(mode.to_string())
	// 				.value(mode.to_string())
	// 				.on_update(move |_| SelectToolMessage::SelectOptions(SelectOptionsUpdate::NestedSelectionBehavior(*mode)).into())
	// 		})
	// 		.collect();

	// 	DropdownInput::new(vec![layer_selection_behavior_entries])
	// 		.selected_index(Some((self.tool_data.nested_selection_behavior == NestedSelectionBehavior::Shallowest) as u32))
	// 		.tooltip("Choose if clicking nested layers directly selects the deepest, or selects the shallowest and deepens by double clicking")
	// 		.widget_holder()
	// }

	fn pivot_widget(&self, disabled: bool) -> WidgetHolder {
		PivotInput::new(self.tool_data.pivot.to_pivot_position())
			.on_update(|pivot_input: &PivotInput| SelectToolMessage::SetPivot { position: pivot_input.position }.into())
			.disabled(disabled)
			.widget_holder()
	}

	fn alignment_widgets(&self, disabled: bool) -> impl Iterator<Item = WidgetHolder> {
		[AlignAxis::X, AlignAxis::Y]
			.into_iter()
			.flat_map(|axis| [(axis, AlignAggregate::Min), (axis, AlignAggregate::Center), (axis, AlignAggregate::Max)])
			.map(move |(axis, aggregate)| {
				let (icon, tooltip) = match (axis, aggregate) {
					(AlignAxis::X, AlignAggregate::Min) => ("AlignLeft", "Align Left"),
					(AlignAxis::X, AlignAggregate::Center) => ("AlignHorizontalCenter", "Align Horizontal Center"),
					(AlignAxis::X, AlignAggregate::Max) => ("AlignRight", "Align Right"),
					(AlignAxis::Y, AlignAggregate::Min) => ("AlignTop", "Align Top"),
					(AlignAxis::Y, AlignAggregate::Center) => ("AlignVerticalCenter", "Align Vertical Center"),
					(AlignAxis::Y, AlignAggregate::Max) => ("AlignBottom", "Align Bottom"),
				};
				IconButton::new(icon, 24)
					.tooltip(tooltip)
					.on_update(move |_| DocumentMessage::AlignSelectedLayers { axis, aggregate }.into())
					.disabled(disabled)
					.widget_holder()
			})
	}

	fn flip_widgets(&self, disabled: bool) -> impl Iterator<Item = WidgetHolder> {
		[(FlipAxis::X, "Horizontal"), (FlipAxis::Y, "Vertical")].into_iter().map(move |(flip_axis, name)| {
			IconButton::new("Flip".to_string() + name, 24)
				.tooltip("Flip ".to_string() + name)
				.on_update(move |_| DocumentMessage::FlipSelectedLayers { flip_axis }.into())
				.disabled(disabled)
				.widget_holder()
		})
	}

	fn boolean_widgets(&self) -> impl Iterator<Item = WidgetHolder> {
		["Union", "Subtract Front", "Subtract Back", "Intersect", "Difference"].into_iter().map(|name| {
			IconButton::new(format!("Boolean{}", name.replace(' ', "")), 24)
				.tooltip(format!("Boolean {name} (coming soon)"))
				.on_update(|_| DialogMessage::RequestComingSoonDialog { issue: Some(1091) }.into())
				.widget_holder()
		})
	}
}

impl LayoutHolder for SelectTool {
	fn layout(&self) -> Layout {
		let mut widgets = Vec::new();
		// widgets.push(self.deep_selection_widget()); // TODO: Reenable once Deep/Shallow Selection is implemented again

		// Pivot
		// widgets.push(Separator::new(SeparatorType::Related).widget_holder()); // TODO: Reenable once Deep/Shallow Selection is implemented again
		widgets.push(self.pivot_widget(self.tool_data.selected_layers_count == 0));

		// Align
		let disabled = self.tool_data.selected_layers_count < 2;
		widgets.push(Separator::new(SeparatorType::Section).widget_holder());
		widgets.extend(self.alignment_widgets(disabled));
		widgets.push(Separator::new(SeparatorType::Related).widget_holder());
		widgets.push(PopoverButton::new("Align", "Coming soon").disabled(disabled).widget_holder());

		// Flip
		let disabled = self.tool_data.selected_layers_count == 0;
		widgets.push(Separator::new(SeparatorType::Section).widget_holder());
		widgets.extend(self.flip_widgets(disabled));
		widgets.push(Separator::new(SeparatorType::Related).widget_holder());
		widgets.push(PopoverButton::new("Flip", "Coming soon").disabled(disabled).widget_holder());

		// Boolean
		if self.tool_data.selected_layers_count >= 2 {
			widgets.push(Separator::new(SeparatorType::Section).widget_holder());
			widgets.extend(self.boolean_widgets());
			widgets.push(Separator::new(SeparatorType::Related).widget_holder());
			widgets.push(PopoverButton::new("Boolean", "Coming soon").widget_holder());
		}

		Layout::WidgetLayout(WidgetLayout::new(vec![LayoutGroup::Row { widgets }]))
	}
}

impl<'a> MessageHandler<ToolMessage, &mut ToolActionHandlerData<'a>> for SelectTool {
	fn process_message(&mut self, message: ToolMessage, responses: &mut VecDeque<Message>, tool_data: &mut ToolActionHandlerData<'a>) {
		if let ToolMessage::Select(SelectToolMessage::SelectOptions(SelectOptionsUpdate::NestedSelectionBehavior(nested_selection_behavior))) = message {
			self.tool_data.nested_selection_behavior = nested_selection_behavior;
			responses.add(ToolMessage::UpdateHints);
		}

		self.fsm_state.process_event(message, &mut self.tool_data, tool_data, &(), responses, false);

		if self.tool_data.pivot.should_refresh_pivot_position() || self.tool_data.selected_layers_changed {
			// Send the layout containing the updated pivot position (a bit ugly to do it here not in the fsm but that doesn't have SelectTool)
			self.send_layout(responses, LayoutTarget::ToolOptions);
			self.tool_data.selected_layers_changed = false;
		}
	}

	fn actions(&self) -> ActionList {
		use SelectToolFsmState::*;

		match self.fsm_state {
			Ready => actions!(SelectToolMessageDiscriminant;
				DragStart,
				PointerMove,
				Abort,
				EditLayer,
				Enter,
			),
			_ => actions!(SelectToolMessageDiscriminant;
				DragStop,
				PointerMove,
				Abort,
				EditLayer,
				Enter,
			),
		}
	}
}

impl ToolTransition for SelectTool {
	fn event_to_message_map(&self) -> EventToMessageMap {
		EventToMessageMap {
			document_dirty: Some(SelectToolMessage::DocumentIsDirty.into()),
			tool_abort: Some(SelectToolMessage::Abort.into()),
			selection_changed: Some(SelectToolMessage::SelectionChanged.into()),
			..Default::default()
		}
	}
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
enum SelectToolFsmState {
	#[default]
	Ready,
	Dragging,
	DrawingBox,
	ResizingBounds,
	RotatingBounds,
	DraggingPivot,
}

#[derive(Clone, Debug, Default)]
struct SelectToolData {
	drag_start: ViewportPosition,
	drag_current: ViewportPosition,
	layers_dragging: Vec<LayerNodeIdentifier>,
	layer_selected_on_start: Option<LayerNodeIdentifier>,
	select_single_layer: Option<LayerNodeIdentifier>,
	has_dragged: bool,
	not_duplicated_layers: Option<Vec<LayerNodeIdentifier>>,
	drag_box_overlay_layer: Option<Vec<LayerId>>,
	path_outlines: PathOutline,
	bounding_box_overlays: Option<BoundingBoxOverlays>,
	snap_manager: SnapManager,
	cursor: MouseCursorIcon,
	pivot: Pivot,
	nested_selection_behavior: NestedSelectionBehavior,
	selected_layers_count: usize,
	selected_layers_changed: bool,
}

impl SelectToolData {
	fn selection_quad(&self) -> Quad {
		let bbox = self.selection_box();
		Quad::from_box(bbox)
	}

	fn selection_box(&self) -> [DVec2; 2] {
		if self.drag_current == self.drag_start {
			let tolerance = DVec2::splat(SELECTION_TOLERANCE);
			[self.drag_start - tolerance, self.drag_start + tolerance]
		} else {
			[self.drag_start, self.drag_current]
		}
	}

	/// Duplicates the currently dragging layers. Called when Alt is pressed and the layers have not yet been duplicated.
	fn start_duplicates(&mut self, document: &DocumentMessageHandler, responses: &mut VecDeque<Message>) {
		responses.add(DocumentMessage::DeselectAllLayers);

		// Take the selected layers and store them in a separate list.
		self.not_duplicated_layers = Some(self.layers_dragging.clone());

		// Duplicate each previously selected layer and select the new ones.
		for layer_ancestors in document.metadata().shallowest_unique_layers(self.layers_dragging.iter().copied()) {
			let layer = layer_ancestors.last().unwrap();
			// Moves the original back to its starting position.
			responses.add_front(GraphOperationMessage::TransformChange {
				layer: layer.to_path(),
				transform: DAffine2::from_translation(self.drag_start - self.drag_current),
				transform_in: TransformIn::Viewport,
				skip_rerender: true,
			});

			// Copy the layers.
			// Not using the Copy message allows us to retrieve the ids of the new layers to initialize the drag.
			todo!();
			// let layer = match document.document_legacy.layer(layer_path) {
			// 	Ok(layer) => layer.clone(),
			// 	Err(e) => {
			// 		warn!("Could not access selected layer {layer_path:?}: {e:?}");
			// 		continue;
			// 	}
			// };

			// let layer_metadata = *document.layer_metadata(layer_path);
			// *layer_path.last_mut().unwrap() = generate_uuid();

			// responses.add(Operation::InsertLayer {
			// 	layer: Box::new(layer),
			// 	destination_path: layer_path.clone(),
			// 	insert_index: -1,
			// 	duplicating: false,
			// });
			// responses.add(DocumentMessage::UpdateLayerMetadata {
			// 	layer_path: layer_path.clone(),
			// 	layer_metadata,
			// });
		}

		// // Since the selected layers have now moved back to their original transforms before the drag began, we rerender them to be displayed as if they weren't touched.
		// for layer_path in self.not_duplicated_layers.iter().flatten() {
		// 	responses.add(DocumentMessage::InputFrameRasterizeRegionBelowLayer { layer_path: layer_path.clone() });
		// }
	}

	/// Removes the duplicated layers. Called when Alt is released and the layers have previously been duplicated.
	fn stop_duplicates(&mut self, document: &DocumentMessageHandler, responses: &mut VecDeque<Message>) {
		let Some(originals) = self.not_duplicated_layers.take() else {
			return;
		};

		responses.add(DocumentMessage::DeselectAllLayers);

		// Delete the duplicated layers
		for layer_ancestors in document.metadata().shallowest_unique_layers(self.layers_dragging.iter().copied()) {
			responses.add(GraphOperationMessage::DeleteLayer {
				id: layer_ancestors.last().unwrap().to_node(),
			});
		}

		// Move the original to under the mouse
		for layer_ancestors in document.metadata().shallowest_unique_layers(originals.iter().copied()) {
			responses.add_front(GraphOperationMessage::TransformChange {
				layer: layer_ancestors.last().unwrap().to_path(),
				transform: DAffine2::from_translation(self.drag_current - self.drag_start),
				transform_in: TransformIn::Viewport,
				skip_rerender: true,
			});
		}

		// Select the originals
		responses.add(NodeGraphMessage::SelectedNodesSet {
			nodes: originals.iter().map(|layer| layer.to_node()).collect::<Vec<_>>(),
		});

		self.layers_dragging = originals;
	}
}

impl Fsm for SelectToolFsmState {
	type ToolData = SelectToolData;
	type ToolOptions = ();

	fn transition(self, event: ToolMessage, tool_data: &mut Self::ToolData, tool_action_data: &mut ToolActionHandlerData, _tool_options: &(), responses: &mut VecDeque<Message>) -> Self {
		let ToolActionHandlerData { document, input, render_data, .. } = tool_action_data;

		let ToolMessage::Select(event) = event else {
			return self;
		};
		match (self, event) {
			(_, SelectToolMessage::DocumentIsDirty | SelectToolMessage::SelectionChanged) => {
				let selected_layers_count = document.metadata().selected_layers().count();
				tool_data.selected_layers_changed = selected_layers_count != tool_data.selected_layers_count;
				tool_data.selected_layers_count = selected_layers_count;

				tool_data.path_outlines.update_selected(document.document_legacy.selected_visible_layers(), document, responses);
				tool_data.path_outlines.intersect_test_hovered(input, document, responses);

				match (document.document_legacy.selected_visible_layers_bounding_box_viewport(), tool_data.bounding_box_overlays.take()) {
					(None, Some(bounding_box_overlays)) => bounding_box_overlays.delete(responses),
					(Some(bounds), paths) => {
						let mut bounding_box_overlays = paths.unwrap_or_else(|| BoundingBoxOverlays::new(responses));

						bounding_box_overlays.bounds = bounds;
						bounding_box_overlays.transform = DAffine2::IDENTITY;

						bounding_box_overlays.transform(responses);

						tool_data.bounding_box_overlays = Some(bounding_box_overlays);
					}
					(_, _) => {}
				};

				tool_data.pivot.update_pivot(document, responses);

				self
			}
			(_, SelectToolMessage::EditLayer) => {
				// Edit the clicked layer
				if let Some(intersect) = document.document_legacy.click(input.mouse.position, &document.document_legacy.document_network) {
					match tool_data.nested_selection_behavior {
						NestedSelectionBehavior::Shallowest => edit_layer_shallowest_manipulation(document, intersect, responses),
						NestedSelectionBehavior::Deepest => edit_layer_deepest_manipulation(intersect, &document.document_legacy, responses),
					}
				}

				self
			}
			(SelectToolFsmState::Ready, SelectToolMessage::DragStart { add_to_selection, select_deepest: _ }) => {
				tool_data.path_outlines.clear_hovered(responses);

				tool_data.drag_start = input.mouse.position;
				tool_data.drag_current = input.mouse.position;

				let dragging_bounds = tool_data.bounding_box_overlays.as_mut().and_then(|bounding_box| {
					let edges = bounding_box.check_selected_edges(input.mouse.position);

					bounding_box.selected_edges = edges.map(|(top, bottom, left, right)| {
						let selected_edges = SelectedEdges::new(top, bottom, left, right, bounding_box.bounds);
						bounding_box.opposite_pivot = selected_edges.calculate_pivot();
						selected_edges
					});

					edges
				});

				let rotating_bounds = tool_data
					.bounding_box_overlays
					.as_ref()
					.map(|bounding_box| bounding_box.check_rotate(input.mouse.position))
					.unwrap_or_default();

				let mut selected: Vec<_> = document.document_legacy.selected_visible_layers().collect();
				let intersection = document.document_legacy.click(input.mouse.position, &document.document_legacy.document_network);

				// If the user is dragging the bounding box bounds, go into ResizingBounds mode.
				// If the user is dragging the rotate trigger, go into RotatingBounds mode.
				// If the user clicks on a layer that is in their current selection, go into the dragging mode.
				// If the user clicks on new shape, make that layer their new selection.
				// Otherwise enter the box select mode
				let state = if tool_data.pivot.is_over(input.mouse.position) {
					responses.add(DocumentMessage::StartTransaction);

					tool_data.snap_manager.start_snap(document, input, document.bounding_boxes(None, None, render_data), true, true);
					tool_data.snap_manager.add_all_document_handles(document, input, &[], &[], &[]);

					SelectToolFsmState::DraggingPivot
				} else if let Some(_selected_edges) = dragging_bounds {
					responses.add(DocumentMessage::StartTransaction);

					// let snap_x = selected_edges.2 || selected_edges.3;
					// let snap_y = selected_edges.0 || selected_edges.1;
					//
					// tool_data
					// 	.snap_manager
					// 	.start_snap(document, input, document.bounding_boxes(Some(&selected), None, render_data), snap_x, snap_y);
					// tool_data
					// 	.snap_manager
					// 	.add_all_document_handles(document, input, &[], &selected.iter().map(|x| x.as_slice()).collect::<Vec<_>>(), &[]);

					tool_data.layers_dragging = selected;

					if let Some(bounds) = &mut tool_data.bounding_box_overlays {
						let document = &document.document_legacy;

						let mut selected = Selected::new(
							&mut bounds.original_transforms,
							&mut bounds.center_of_transformation,
							&tool_data.layers_dragging,
							responses,
							document,
							None,
							&ToolType::Select,
						);
						bounds.center_of_transformation = selected.mean_average_of_pivots();
					}

					SelectToolFsmState::ResizingBounds
				} else if rotating_bounds {
					responses.add(DocumentMessage::StartTransaction);

					if let Some(bounds) = &mut tool_data.bounding_box_overlays {
						let mut selected = Selected::new(
							&mut bounds.original_transforms,
							&mut bounds.center_of_transformation,
							&selected,
							responses,
							&document.document_legacy,
							None,
							&ToolType::Select,
						);

						bounds.center_of_transformation = selected.mean_average_of_pivots();
					}

					tool_data.layers_dragging = selected;

					SelectToolFsmState::RotatingBounds
				} else if intersection.is_some_and(|intersection| selected.iter().any(|selected_layer| intersection.starts_with(*selected_layer, document.metadata())))
					&& tool_data.nested_selection_behavior == NestedSelectionBehavior::Deepest
				{
					responses.add(DocumentMessage::StartTransaction);
					tool_data.select_single_layer = intersection;

					tool_data.layers_dragging = selected;

					// tool_data
					// 	.snap_manager
					// 	.start_snap(document, input, document.bounding_boxes(Some(&tool_data.layers_dragging), None, render_data), true, true);

					SelectToolFsmState::Dragging
				} else {
					responses.add(DocumentMessage::StartTransaction);
					tool_data.layers_dragging = selected;

					if !input.keyboard.key(add_to_selection) && tool_data.nested_selection_behavior == NestedSelectionBehavior::Deepest {
						responses.add(DocumentMessage::DeselectAllLayers);
						tool_data.layers_dragging.clear();
					}

					if let Some(intersection) = intersection {
						tool_data.layer_selected_on_start = Some(intersection);
						selected = vec![intersection];

						match tool_data.nested_selection_behavior {
							NestedSelectionBehavior::Shallowest => drag_shallowest_manipulation(responses, selected, tool_data, document),
							NestedSelectionBehavior::Deepest => drag_deepest_manipulation(responses, selected, tool_data),
						}
						SelectToolFsmState::Dragging
					} else {
						// Deselect all layers if using shallowest selection behavior
						// Necessary since for shallowest mode, we need to know the current selected layers to determine the next
						if tool_data.nested_selection_behavior == NestedSelectionBehavior::Shallowest {
							responses.add(DocumentMessage::DeselectAllLayers);
							tool_data.layers_dragging.clear();
						}
						tool_data.drag_box_overlay_layer = Some(add_bounding_box(responses));
						SelectToolFsmState::DrawingBox
					}
				};
				tool_data.not_duplicated_layers = None;

				state
			}
			(SelectToolFsmState::Dragging, SelectToolMessage::PointerMove { axis_align, duplicate, .. }) => {
				tool_data.has_dragged = true;
				// TODO: This is a cheat. Break out the relevant functionality from the handler above and call it from there and here.
				responses.add_front(SelectToolMessage::DocumentIsDirty);

				let mouse_position = axis_align_drag(input.keyboard.key(axis_align), input.mouse.position, tool_data.drag_start);

				let mouse_delta = mouse_position - tool_data.drag_current;

				let snap = tool_data
					.layers_dragging
					.iter()
					.filter_map(|&layer| document.metadata().bounding_box_viewport(layer))
					.flat_map(snapping::expand_bounds)
					.collect();

				let closest_move = tool_data.snap_manager.snap_layers(responses, document, snap, mouse_delta);
				// TODO: Cache the result of `shallowest_unique_layers` to avoid this heavy computation every frame of movement, see https://github.com/GraphiteEditor/Graphite/pull/481
				for layer_ancestors in document.metadata().shallowest_unique_layers(tool_data.layers_dragging.iter().copied()) {
					responses.add_front(GraphOperationMessage::TransformChange {
						layer: layer_ancestors.last().unwrap().to_path(),
						transform: DAffine2::from_translation(mouse_delta + closest_move),
						transform_in: TransformIn::Viewport,
						skip_rerender: false,
					});
				}
				tool_data.drag_current = mouse_position + closest_move;

				if input.keyboard.key(duplicate) && tool_data.not_duplicated_layers.is_none() {
					tool_data.start_duplicates(document, responses);
				} else if !input.keyboard.key(duplicate) && tool_data.not_duplicated_layers.is_some() {
					tool_data.stop_duplicates(document, responses);
				}

				SelectToolFsmState::Dragging
			}
			(SelectToolFsmState::ResizingBounds, SelectToolMessage::PointerMove { axis_align, center, .. }) => {
				if let Some(bounds) = &mut tool_data.bounding_box_overlays {
					if let Some(movement) = &mut bounds.selected_edges {
						let (center, axis_align) = (input.keyboard.key(center), input.keyboard.key(axis_align));

						let mouse_position = input.mouse.position;

						let snapped_mouse_position = tool_data.snap_manager.snap_position(responses, document, mouse_position);

						let (position, size) = movement.new_size(snapped_mouse_position, bounds.transform, center, bounds.center_of_transformation, axis_align);
						let (delta, mut _pivot) = movement.bounds_to_scale_transform(position, size);

						let selected = &tool_data.layers_dragging;
						let mut selected = Selected::new(&mut bounds.original_transforms, &mut _pivot, selected, responses, &document.document_legacy, None, &ToolType::Select);

						selected.update_transforms(delta);
					}
				}
				SelectToolFsmState::ResizingBounds
			}
			(SelectToolFsmState::RotatingBounds, SelectToolMessage::PointerMove { snap_angle, .. }) => {
				if let Some(bounds) = &mut tool_data.bounding_box_overlays {
					let angle = {
						let start_offset = tool_data.drag_start - bounds.center_of_transformation;
						let end_offset = input.mouse.position - bounds.center_of_transformation;

						start_offset.angle_between(end_offset)
					};

					let snapped_angle = if input.keyboard.key(snap_angle) {
						let snap_resolution = ROTATE_SNAP_ANGLE.to_radians();
						(angle / snap_resolution).round() * snap_resolution
					} else {
						angle
					};

					let delta = DAffine2::from_angle(snapped_angle);

					let mut selected = Selected::new(
						&mut bounds.original_transforms,
						&mut bounds.center_of_transformation,
						&tool_data.layers_dragging,
						responses,
						&document.document_legacy,
						None,
						&ToolType::Select,
					);

					selected.update_transforms(delta);
				}

				SelectToolFsmState::RotatingBounds
			}
			(SelectToolFsmState::DraggingPivot, SelectToolMessage::PointerMove { .. }) => {
				let mouse_position = input.mouse.position;
				let snapped_mouse_position = tool_data.snap_manager.snap_position(responses, document, mouse_position);
				tool_data.pivot.set_viewport_position(snapped_mouse_position, document, responses);

				SelectToolFsmState::DraggingPivot
			}
			(SelectToolFsmState::DrawingBox, SelectToolMessage::PointerMove { .. }) => {
				tool_data.drag_current = input.mouse.position;

				responses.add_front(DocumentMessage::Overlays(
					Operation::SetLayerTransformInViewport {
						path: tool_data.drag_box_overlay_layer.clone().unwrap(),
						transform: transform_from_box(tool_data.drag_start, tool_data.drag_current, DAffine2::IDENTITY).to_cols_array(),
					}
					.into(),
				));
				SelectToolFsmState::DrawingBox
			}
			(SelectToolFsmState::Ready, SelectToolMessage::PointerMove { .. }) => {
				let mut cursor = tool_data.bounding_box_overlays.as_ref().map_or(MouseCursorIcon::Default, |bounds| bounds.get_cursor(input, true));

				// Dragging the pivot overrules the other operations
				if tool_data.pivot.is_over(input.mouse.position) {
					cursor = MouseCursorIcon::Move;
				}

				// Generate the select outline (but not if the user is going to use the bound overlays)
				if cursor == MouseCursorIcon::Default {
					tool_data.path_outlines.intersect_test_hovered(input, document, responses);
				} else {
					tool_data.path_outlines.clear_hovered(responses);
				}

				if tool_data.cursor != cursor {
					tool_data.cursor = cursor;
					responses.add(FrontendMessage::UpdateMouseCursor { cursor });
				}

				SelectToolFsmState::Ready
			}
			(SelectToolFsmState::Dragging, SelectToolMessage::Enter) => {
				let response = match input.mouse.position.distance(tool_data.drag_start) < 10. * f64::EPSILON {
					true => DocumentMessage::Undo,
					false => DocumentMessage::CommitTransaction,
				};
				tool_data.snap_manager.cleanup(responses);
				responses.add_front(response);

				SelectToolFsmState::Ready
			}
			(SelectToolFsmState::Dragging, SelectToolMessage::DragStop { remove_from_selection }) => {
				// Deselect layer if not snap dragging
				if !tool_data.has_dragged && input.keyboard.key(remove_from_selection) && tool_data.layer_selected_on_start.is_none() {
					let quad = tool_data.selection_quad();
					let intersection = document.document_legacy.intersect_quad(quad, &document.document_legacy.document_network);

					if let Some(path) = intersection.last() {
						let replacement_selected_layers: Vec<_> = document.metadata().selected_layers().filter(|&layer| !path.starts_with(layer, document.metadata())).collect();

						tool_data.layers_dragging.clear();
						tool_data.layers_dragging.extend(replacement_selected_layers.iter());

						responses.add(NodeGraphMessage::SelectedNodesSet {
							nodes: replacement_selected_layers.iter().map(|layer| layer.to_node()).collect(),
						});
					}
				} else if let Some(selecting_layer) = tool_data.select_single_layer.take() {
					if !tool_data.has_dragged {
						responses.add(NodeGraphMessage::SelectedNodesSet {
							nodes: vec![selecting_layer.to_node()],
						});
					}
				}

				tool_data.has_dragged = false;
				tool_data.layer_selected_on_start = None;

				responses.add(DocumentMessage::CommitTransaction);
				tool_data.snap_manager.cleanup(responses);
				tool_data.select_single_layer = None;

				SelectToolFsmState::Ready
			}
			(SelectToolFsmState::ResizingBounds, SelectToolMessage::DragStop { .. } | SelectToolMessage::Enter) => {
				let response = match input.mouse.position.distance(tool_data.drag_start) < 10. * f64::EPSILON {
					true => DocumentMessage::Undo,
					false => DocumentMessage::CommitTransaction,
				};
				responses.add(response);

				tool_data.snap_manager.cleanup(responses);

				if let Some(bounds) = &mut tool_data.bounding_box_overlays {
					bounds.original_transforms.clear();
				}

				SelectToolFsmState::Ready
			}
			(SelectToolFsmState::RotatingBounds, SelectToolMessage::DragStop { .. } | SelectToolMessage::Enter) => {
				let response = match input.mouse.position.distance(tool_data.drag_start) < 10. * f64::EPSILON {
					true => DocumentMessage::Undo,
					false => DocumentMessage::CommitTransaction,
				};
				responses.add(response);

				if let Some(bounds) = &mut tool_data.bounding_box_overlays {
					bounds.original_transforms.clear();
				}

				SelectToolFsmState::Ready
			}
			(SelectToolFsmState::DraggingPivot, SelectToolMessage::DragStop { .. } | SelectToolMessage::Enter) => {
				let response = match input.mouse.position.distance(tool_data.drag_start) < 10. * f64::EPSILON {
					true => DocumentMessage::Undo,
					false => DocumentMessage::CommitTransaction,
				};
				responses.add(response);

				tool_data.snap_manager.cleanup(responses);

				SelectToolFsmState::Ready
			}
			(SelectToolFsmState::DrawingBox, SelectToolMessage::DragStop { .. } | SelectToolMessage::Enter) => {
				let quad = tool_data.selection_quad();
				// For shallow select we don't update dragging layers until inside drag_start_shallowest_manipulation()
				tool_data.layers_dragging = document.document_legacy.intersect_quad(quad, &document.document_legacy.document_network).collect();
				responses.add_front(NodeGraphMessage::SelectedNodesSet {
					nodes: tool_data.layers_dragging.iter().map(|layer| layer.to_node()).collect(),
				});
				responses.add_front(DocumentMessage::Overlays(
					Operation::DeleteLayer {
						path: tool_data.drag_box_overlay_layer.take().unwrap(),
					}
					.into(),
				));
				SelectToolFsmState::Ready
			}
			(SelectToolFsmState::Ready, SelectToolMessage::Enter) => {
				let mut selected_layers = document.metadata().selected_layers();

				if let Some(layer) = selected_layers.next() {
					// Check that only one layer is selected
					if selected_layers.next().is_none() && is_text_layer(layer, &document.document_legacy) {
						responses.add_front(ToolMessage::ActivateTool { tool_type: ToolType::Text });
						responses.add(TextToolMessage::EditSelected);
					}
				}

				SelectToolFsmState::Ready
			}
			(SelectToolFsmState::Dragging, SelectToolMessage::Abort) => {
				tool_data.snap_manager.cleanup(responses);
				responses.add(DocumentMessage::Undo);

				tool_data.path_outlines.clear_selected(responses);
				tool_data.pivot.clear_overlays(responses);

				SelectToolFsmState::Ready
			}
			(_, SelectToolMessage::Abort) => {
				if let Some(path) = tool_data.drag_box_overlay_layer.take() {
					responses.add_front(DocumentMessage::Overlays(Operation::DeleteLayer { path }.into()))
				};
				if let Some(mut bounding_box_overlays) = tool_data.bounding_box_overlays.take() {
					let mut selected = Selected::new(
						&mut bounding_box_overlays.original_transforms,
						&mut bounding_box_overlays.opposite_pivot,
						&tool_data.layers_dragging,
						responses,
						&document.document_legacy,
						None,
						&ToolType::Select,
					);

					selected.revert_operation();

					bounding_box_overlays.delete(responses);
				}

				tool_data.path_outlines.clear_hovered(responses);
				tool_data.path_outlines.clear_selected(responses);
				tool_data.pivot.clear_overlays(responses);

				tool_data.snap_manager.cleanup(responses);
				SelectToolFsmState::Ready
			}
			(_, SelectToolMessage::SetPivot { position }) => {
				responses.add(DocumentMessage::StartTransaction);

				let pos: Option<DVec2> = position.into();
				tool_data.pivot.set_normalized_position(pos.unwrap(), document, responses);

				self
			}
			_ => self,
		}
	}

	fn standard_tool_messages(&self, message: &ToolMessage, responses: &mut VecDeque<Message>, tool_data: &mut Self::ToolData) -> bool {
		// Check for standard hits or cursor events
		match message {
			ToolMessage::UpdateHints => {
				let hint_data = HintData(vec![
					HintGroup(vec![HintInfo::mouse(MouseMotion::LmbDrag, "Drag Selected")]),
					HintGroup(vec![HintInfo::keys([Key::KeyG, Key::KeyR, Key::KeyS], "Grab/Rotate/Scale Selected")]),
					HintGroup({
						let mut hints = vec![HintInfo::mouse(MouseMotion::Lmb, "Select Object"), HintInfo::keys([Key::Shift], "Extend Selection").prepend_plus()];
						if tool_data.nested_selection_behavior == NestedSelectionBehavior::Shallowest {
							hints.extend([HintInfo::keys([Key::Accel], "Deepest").prepend_plus(), HintInfo::mouse(MouseMotion::LmbDouble, "Deepen Selection")]);
						}
						hints
					}),
					HintGroup(vec![
						HintInfo::mouse(MouseMotion::LmbDrag, "Select Area"),
						HintInfo::keys([Key::Shift], "Extend Selection").prepend_plus(),
					]),
					HintGroup(vec![
						HintInfo::arrow_keys("Nudge Selected"),
						HintInfo::keys([Key::Shift], "10x").prepend_plus(),
						HintInfo::keys([Key::Alt], "Resize Corner").prepend_plus(),
						HintInfo::keys([Key::Control], "Opp. Corner").prepend_plus(),
					]),
					HintGroup(vec![
						HintInfo::keys_and_mouse([Key::Alt], MouseMotion::LmbDrag, "Move Duplicate"),
						HintInfo::keys([Key::Control, Key::KeyD], "Duplicate").add_mac_keys([Key::Command, Key::KeyD]),
					]),
				]);

				responses.add(FrontendMessage::UpdateInputHints { hint_data });
				self.update_hints(responses);
				true
			}
			ToolMessage::UpdateCursor => {
				self.update_cursor(responses);
				true
			}
			_ => false,
		}
	}

	fn update_hints(&self, _responses: &mut VecDeque<Message>) {}

	fn update_cursor(&self, responses: &mut VecDeque<Message>) {
		responses.add(FrontendMessage::UpdateMouseCursor { cursor: MouseCursorIcon::Default });
	}
}

fn drag_shallowest_manipulation(responses: &mut VecDeque<Message>, selected: Vec<LayerNodeIdentifier>, tool_data: &mut SelectToolData, document: &DocumentMessageHandler) {
	let layer = selected[0];
	let ancestor = layer.ancestors(document.metadata()).find(|&ancestor| document.metadata().selected_layers_contains(ancestor));

	let new_selected = ancestor.unwrap_or_else(|| layer.child_of_root(document.metadata()));

	tool_data.layers_dragging = vec![new_selected];
	responses.add(NodeGraphMessage::SelectedNodesSet {
		nodes: tool_data.layers_dragging.iter().map(|layer| layer.to_node()).collect(),
	});
	// tool_data
	// 	.snap_manager
	// 	.start_snap(document, input, document.bounding_boxes(Some(&tool_data.layers_dragging), None, render_data), true, true);
}

fn drag_deepest_manipulation(responses: &mut VecDeque<Message>, mut selected: Vec<LayerNodeIdentifier>, tool_data: &mut SelectToolData) {
	tool_data.layers_dragging.append(&mut selected);
	responses.add(NodeGraphMessage::SelectedNodesSet {
		nodes: tool_data.layers_dragging.iter().map(|layer| layer.to_node()).collect(),
	});
	// tool_data
	// 	.snap_manager
	// 	.start_snap(document, input, document.bounding_boxes(Some(&tool_data.layers_dragging), None, render_data), true, true);
}

fn edit_layer_shallowest_manipulation(document: &DocumentMessageHandler, layer: LayerNodeIdentifier, responses: &mut VecDeque<Message>) {
	if document.metadata().selected_layers_contains(layer) {
		responses.add_front(ToolMessage::ActivateTool { tool_type: ToolType::Path });
		return;
	}

	let Some(new_selected) = layer
		.ancestors(document.metadata())
		.find(|ancestor| ancestor.parent(document.metadata()).is_some_and(|parent| document.metadata().selected_layers_contains(parent)))
	else {
		return;
	};

	responses.add(NodeGraphMessage::SelectedNodesSet { nodes: vec![new_selected.to_node()] });
}

fn edit_layer_deepest_manipulation(layer: LayerNodeIdentifier, document: &Document, responses: &mut VecDeque<Message>) {
	if is_text_layer(layer, document) {
		responses.add_front(ToolMessage::ActivateTool { tool_type: ToolType::Text });
		responses.add(TextToolMessage::EditSelected);
	} else if is_shape_layer(layer, document) {
		responses.add_front(ToolMessage::ActivateTool { tool_type: ToolType::Path });
	}
}
