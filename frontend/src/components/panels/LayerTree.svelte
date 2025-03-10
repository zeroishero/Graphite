<script lang="ts">
	import { getContext, onMount, tick } from "svelte";

	import { beginDraggingElement } from "@graphite/io-managers/drag";
	import { platformIsMac } from "@graphite/utility-functions/platform";
	import type { Editor } from "@graphite/wasm-communication/editor";
	import { defaultWidgetLayout, patchWidgetLayout, UpdateDocumentLayerDetails, UpdateDocumentLayerTreeStructureJs, UpdateLayerTreeOptionsLayout } from "@graphite/wasm-communication/messages";
	import type { LayerType, LayerPanelEntry } from "@graphite/wasm-communication/messages";

	import LayoutCol from "@graphite/components/layout/LayoutCol.svelte";
	import LayoutRow from "@graphite/components/layout/LayoutRow.svelte";
	import IconButton from "@graphite/components/widgets/buttons/IconButton.svelte";
	import IconLabel from "@graphite/components/widgets/labels/IconLabel.svelte";
	import WidgetLayout from "@graphite/components/widgets/WidgetLayout.svelte";

	type LayerListingInfo = {
		folderIndex: number;
		bottomLayer: boolean;
		editingName: boolean;
		entry: LayerPanelEntry;
	};

	let list: LayoutCol | undefined;

	const RANGE_TO_INSERT_WITHIN_BOTTOM_FOLDER_NOT_ROOT = 20;
	const LAYER_INDENT = 16;
	const INSERT_MARK_MARGIN_LEFT = 4 + 32 + LAYER_INDENT;
	const INSERT_MARK_OFFSET = 2;

	type DraggingData = {
		select?: () => void;
		insertFolder: BigUint64Array;
		insertIndex: number;
		highlightFolder: boolean;
		markerHeight: number;
	};

	const editor = getContext<Editor>("editor");

	// Layer data
	let layerCache = new Map<string, LayerPanelEntry>(); // TODO: replace with BigUint64Array as index
	let layers: LayerListingInfo[] = [];

	// Interactive dragging
	let draggable = true;
	let draggingData: undefined | DraggingData = undefined;
	let fakeHighlight: undefined | BigUint64Array[] = undefined;
	let dragInPanel = false;

	// Layouts
	let layerTreeOptionsLayout = defaultWidgetLayout();

	onMount(() => {
		editor.subscriptions.subscribeJsMessage(UpdateLayerTreeOptionsLayout, (updateLayerTreeOptionsLayout) => {
			patchWidgetLayout(layerTreeOptionsLayout, updateLayerTreeOptionsLayout);
			layerTreeOptionsLayout = layerTreeOptionsLayout;
		});

		editor.subscriptions.subscribeJsMessage(UpdateDocumentLayerTreeStructureJs, (updateDocumentLayerTreeStructure) => {
			rebuildLayerTree(updateDocumentLayerTreeStructure);
		});

		editor.subscriptions.subscribeJsMessage(UpdateDocumentLayerDetails, (updateDocumentLayerDetails) => {
			const targetLayer = updateDocumentLayerDetails.data;
			const targetPath = targetLayer.path;

			updateLayerInTree(targetPath, targetLayer);
		});
	});

	function layerIndent(layer: LayerPanelEntry): string {
		return `${layer.path.length * LAYER_INDENT}px`;
	}

	function markIndent(path: BigUint64Array): string {
		return `${INSERT_MARK_MARGIN_LEFT + path.length * LAYER_INDENT}px`;
	}

	function markTopOffset(height: number): string {
		return `${height}px`;
	}

	function toggleLayerVisibility(path: BigUint64Array) {
		editor.instance.toggleLayerVisibility(path);
	}

	function handleExpandArrowClick(path: BigUint64Array) {
		editor.instance.toggleLayerExpansion(path);
	}

	async function onEditLayerName(listing: LayerListingInfo) {
		if (listing.editingName) return;

		draggable = false;
		listing.editingName = true;
		layers = layers;

		await tick();

		const textInput = (list?.div()?.querySelector("[data-text-input]:not([disabled])") || undefined) as HTMLInputElement | undefined;
		textInput?.select();
	}

	function onEditLayerNameChange(listing: LayerListingInfo, e: Event) {
		// Eliminate duplicate events
		if (!listing.editingName) return;

		draggable = true;

		const name = (e.target as HTMLInputElement | undefined)?.value;
		listing.editingName = false;
		layers = layers;
		if (name) editor.instance.setLayerName(listing.entry.path, name);
	}

	async function onEditLayerNameDeselect(listing: LayerListingInfo) {
		draggable = true;

		listing.editingName = false;
		layers = layers;

		await tick();
		window.getSelection()?.removeAllRanges();
	}

	function selectLayerWithModifiers(e: MouseEvent, listing: LayerListingInfo) {
		// Get the pressed state of the modifier keys
		const [ctrl, meta, shift, alt] = [e.ctrlKey, e.metaKey, e.shiftKey, e.altKey];
		// Get the state of the platform's accel key and its opposite platform's accel key
		const [accel, oppositeAccel] = platformIsMac() ? [meta, ctrl] : [ctrl, meta];

		// Select the layer only if the accel and/or shift keys are pressed
		if (!oppositeAccel && !alt) selectLayer(accel, shift, listing);

		e.stopPropagation();
	}

	function selectLayer(accel: boolean, shift: boolean, listing: LayerListingInfo) {
		// Don't select while we are entering text to rename the layer
		if (listing.editingName) return;

		editor.instance.selectLayer(listing.entry.path, accel, shift);
	}

	async function deselectAllLayers() {
		editor.instance.deselectAllLayers();
	}

	function isGroupOrArtboard(layerType: LayerType) {
		return layerType === "Folder" || layerType === "Artboard";
	}

	function calculateDragIndex(tree: LayoutCol, clientY: number, select?: () => void): DraggingData {
		const treeChildren = tree.div()?.children;
		const treeOffset = tree.div()?.getBoundingClientRect().top;

		// Closest distance to the middle of the row along the Y axis
		let closest = Infinity;

		// Folder to insert into
		let insertFolder = new BigUint64Array();

		// Insert index
		let insertIndex = -1;

		// Whether you are inserting into a folder and should show the folder outline
		let highlightFolder = false;

		let markerHeight = 0;
		let previousHeight = undefined as undefined | number;

		if (treeChildren !== undefined && treeOffset !== undefined) {
			Array.from(treeChildren).forEach((treeChild, index) => {
				const layerComponents = treeChild.getElementsByClassName("layer");
				if (layerComponents.length !== 1) return;
				const child = layerComponents[0];

				const indexAttribute = child.getAttribute("data-index");
				if (!indexAttribute) return;
				const { folderIndex, entry: layer } = layers[parseInt(indexAttribute, 10)];

				const rect = child.getBoundingClientRect();
				const position = rect.top + rect.height / 2;
				const distance = position - clientY;

				// Inserting above current row
				if (distance > 0 && distance < closest) {
					insertFolder = layer.path.slice(0, layer.path.length - 1);
					insertIndex = folderIndex;
					highlightFolder = false;
					closest = distance;
					markerHeight = previousHeight || treeOffset + INSERT_MARK_OFFSET;
				}
				// Inserting below current row
				else if (distance > -closest && distance > -RANGE_TO_INSERT_WITHIN_BOTTOM_FOLDER_NOT_ROOT && distance < 0) {
					insertFolder = isGroupOrArtboard(layer.layerType) ? layer.path : layer.path.slice(0, layer.path.length - 1);
					insertIndex = isGroupOrArtboard(layer.layerType) ? 0 : folderIndex + 1;
					highlightFolder = isGroupOrArtboard("Folder");
					closest = -distance;
					markerHeight = index === treeChildren.length - 1 ? rect.bottom - INSERT_MARK_OFFSET : rect.bottom;
				}
				// Inserting with no nesting at the end of the panel
				else if (closest === Infinity) {
					if (layer.path.length === 1) insertIndex = folderIndex + 1;

					markerHeight = rect.bottom - INSERT_MARK_OFFSET;
				}
				previousHeight = rect.bottom;
			});
		}

		markerHeight -= treeOffset || 0;

		return {
			select,
			insertFolder,
			insertIndex,
			highlightFolder,
			markerHeight,
		};
	}

	async function dragStart(event: DragEvent, listing: LayerListingInfo) {
		const layer = listing.entry;
		dragInPanel = true;
		if (!layer.layerMetadata.selected) {
			fakeHighlight = [layer.path];
		}
		const select = () => {
			if (!layer.layerMetadata.selected) selectLayer(false, false, listing);
		};

		const target = (event.target || undefined) as HTMLElement | undefined;
		const draggingELement = (target?.closest("[data-layer]") || undefined) as HTMLElement | undefined;
		if (draggingELement) beginDraggingElement(draggingELement);

		// Set style of cursor for drag
		if (event.dataTransfer) {
			event.dataTransfer.dropEffect = "move";
			event.dataTransfer.effectAllowed = "move";
		}

		if (list) draggingData = calculateDragIndex(list, event.clientY, select);
	}

	function updateInsertLine(event: DragEvent) {
		// Stop the drag from being shown as cancelled
		event.preventDefault();
		dragInPanel = true;

		if (list) draggingData = calculateDragIndex(list, event.clientY, draggingData?.select);
	}

	async function drop() {
		if (draggingData && dragInPanel) {
			const { select, insertFolder, insertIndex } = draggingData;

			select?.();
			editor.instance.moveLayerInTree(insertFolder, insertIndex);
		}
		draggingData = undefined;
		fakeHighlight = undefined;
		dragInPanel = false;
	}

	function rebuildLayerTree(updateDocumentLayerTreeStructure: UpdateDocumentLayerTreeStructureJs) {
		const layerWithNameBeingEdited = layers.find((layer: LayerListingInfo) => layer.editingName);
		const layerPathWithNameBeingEdited = layerWithNameBeingEdited?.entry.path;
		const layerIdWithNameBeingEdited = layerPathWithNameBeingEdited?.slice(-1)[0];
		const path: bigint[] = [];

		// Clear the layer tree before rebuilding it
		layers = [];

		// Build the new layer tree
		const recurse = (folder: UpdateDocumentLayerTreeStructureJs) => {
			folder.children.forEach((item, index) => {
				// TODO: fix toString
				const layerId = BigInt(item.layerId.toString());
				path.push(layerId);

				const mapping = layerCache.get([path[path.length - 1]].toString());
				if (mapping) {
					mapping.path = new BigUint64Array(path);
					layers.push({
						folderIndex: index,
						bottomLayer: index === folder.children.length - 1,
						entry: mapping,
						editingName: layerIdWithNameBeingEdited === layerId,
					});
				}

				// Call self recursively if there are any children
				if (item.children.length >= 1) recurse(item);

				path.pop();
			});
		};
		recurse(updateDocumentLayerTreeStructure);
		layers = layers;
	}

	function updateLayerInTree(targetPath: BigUint64Array, targetLayer: LayerPanelEntry) {
		const path = targetPath.toString();
		layerCache.set(path, targetLayer);

		const layer = layers.find((layer: LayerListingInfo) => layer.entry.path.toString() === path);
		if (layer) {
			layer.entry = targetLayer;
			layers = layers;
		}
	}
</script>

<LayoutCol class="layer-tree" on:dragleave={() => (dragInPanel = false)}>
	<LayoutRow class="options-bar" scrollableX={true}>
		<WidgetLayout layout={layerTreeOptionsLayout} />
	</LayoutRow>
	<LayoutRow class="layer-tree-rows" scrollableY={true}>
		<LayoutCol class="list" bind:this={list} on:click={() => deselectAllLayers()} on:dragover={(e) => draggable && updateInsertLine(e)} on:dragend={() => draggable && drop()}>
			{#each layers as listing, index (String(listing.entry.path.slice(-1)))}
				<LayoutRow
					class="layer-row"
					classes={{
						"insert-folder": (draggingData?.highlightFolder || false) && draggingData?.insertFolder === listing.entry.path,
					}}
				>
					<LayoutRow class="visibility">
						<IconButton
							action={(e) => (toggleLayerVisibility(listing.entry.path), e?.stopPropagation())}
							size={24}
							icon={listing.entry.visible ? "EyeVisible" : "EyeHidden"}
							tooltip={listing.entry.visible ? "Visible" : "Hidden"}
						/>
					</LayoutRow>

					<div class="indent" style:margin-left={layerIndent(listing.entry)} />

					{#if isGroupOrArtboard(listing.entry.layerType)}
						<button class="expand-arrow" class:expanded={listing.entry.layerMetadata.expanded} on:click|stopPropagation={() => handleExpandArrowClick(listing.entry.path)} tabindex="0" />
					{/if}
					<LayoutRow
						class="layer"
						classes={{
							selected: fakeHighlight ? fakeHighlight.includes(listing.entry.path) : listing.entry.layerMetadata.selected,
						}}
						data-layer={String(listing.entry.path)}
						data-index={index}
						tooltip={listing.entry.tooltip}
						{draggable}
						on:dragstart={(e) => draggable && dragStart(e, listing)}
						on:click={(e) => selectLayerWithModifiers(e, listing)}
					>
						<LayoutRow class="layer-type-icon">
							<IconLabel icon={listing.entry.layerType || "Info"} />
						</LayoutRow>
						<LayoutRow class="layer-name" on:dblclick={() => onEditLayerName(listing)}>
							<input
								data-text-input
								type="text"
								value={listing.entry.name}
								placeholder={`Untitled ${listing.entry.layerType || "[Unknown Layer Type]"}`}
								disabled={!listing.editingName}
								on:blur={() => onEditLayerNameDeselect(listing)}
								on:keydown={(e) => e.key === "Escape" && onEditLayerNameDeselect(listing)}
								on:keydown={(e) => e.key === "Enter" && onEditLayerNameChange(listing, e)}
								on:change={(e) => onEditLayerNameChange(listing, e)}
							/>
						</LayoutRow>
						<div class="thumbnail">
							{@html listing.entry.thumbnail}
						</div>
					</LayoutRow>
				</LayoutRow>
			{/each}
		</LayoutCol>
		{#if draggingData && !draggingData.highlightFolder && dragInPanel}
			<div class="insert-mark" style:left={markIndent(draggingData.insertFolder)} style:top={markTopOffset(draggingData.markerHeight)} />
		{/if}
	</LayoutRow>
</LayoutCol>

<style lang="scss" global>
	.layer-tree {
		// Options bar
		.options-bar {
			height: 32px;
			flex: 0 0 auto;
			margin: 0 4px;
			align-items: center;

			.widget-layout {
				width: 100%;
				min-width: 300px;
			}

			// Blend mode selector
			.dropdown-input {
				max-width: 120px;
			}

			// Blend mode selector and opacity slider
			.dropdown-input,
			.number-input {
				flex: 1 1 auto;
			}
		}

		// Layer tree
		.layer-tree-rows {
			margin-top: 4px;
			// Crop away the 1px border below the bottom layer entry when it uses the full space of this panel
			margin-bottom: -1px;
			position: relative;

			.layer-row {
				flex: 0 0 auto;
				align-items: center;
				position: relative;
				height: 32px;
				margin: 0 4px;
				border-bottom: 1px solid var(--color-4-dimgray);

				.visibility {
					flex: 0 0 auto;
					height: 100%;
					align-items: center;

					.icon-button {
						height: 100%;
						width: calc(24px + 2 * 4px);
					}
				}

				.expand-arrow {
					padding: 0;
					margin: 0;
					margin-left: -16px;
					width: 16px;
					height: 100%;
					border: none;
					position: relative;
					background: none;
					flex: 0 0 auto;
					display: flex;
					align-items: center;
					justify-content: center;
					border-radius: 2px;

					&:hover {
						background: var(--color-5-dullgray);
					}

					&::after {
						content: "";
						position: absolute;
						width: 0;
						height: 0;
						border-style: solid;
						border-width: 3px 0 3px 6px;
						border-color: transparent transparent transparent var(--color-e-nearwhite);
					}

					&.expanded::after {
						border-width: 6px 3px 0 3px;
						border-color: var(--color-e-nearwhite) transparent transparent transparent;
					}
				}

				.layer {
					align-items: center;
					z-index: 1;
					width: 100%;
					height: 100%;
					padding: 0 4px;
					border-radius: 2px;
					margin-right: 8px;

					&.selected {
						background: var(--color-5-dullgray);
					}

					.layer-type-icon {
						flex: 0 0 auto;
						margin: 0 4px;
					}

					.layer-name {
						flex: 1 1 100%;
						margin: 0 4px;

						input {
							color: inherit;
							background: none;
							border: none;
							outline: none; // Ok for input element
							margin: 0;
							padding: 0;
							text-overflow: ellipsis;
							white-space: nowrap;
							overflow: hidden;
							border-radius: 2px;
							height: 24px;
							width: 100%;

							&:disabled {
								-webkit-user-select: none; // Required as of Safari 15.0 (Graphite's minimum version) through the latest release
								user-select: none;
								// Workaround for `user-select: none` not working on <input> elements
								pointer-events: none;
							}

							&::placeholder {
								opacity: 1;
								color: inherit;
								font-style: italic;
							}

							&:focus {
								background: var(--color-1-nearblack);
								padding: 0 4px;

								&::placeholder {
									opacity: 0.5;
								}
							}
						}
					}

					.thumbnail {
						width: 36px;
						height: 24px;
						margin: 2px 0;
						margin-left: 4px;
						background: white;
						border-radius: 2px;
						flex: 0 0 auto;

						svg {
							width: calc(100% - 4px);
							height: calc(100% - 4px);
							margin: 2px;
						}
					}
				}

				&.insert-folder .layer {
					outline: 3px solid var(--color-e-nearwhite);
					outline-offset: -3px;
				}
			}

			.insert-mark {
				position: absolute;
				// `left` is applied dynamically
				right: 0;
				background: var(--color-e-nearwhite);
				margin-top: -2px;
				height: 5px;
				z-index: 1;
				pointer-events: none;
			}
		}
	}
</style>
