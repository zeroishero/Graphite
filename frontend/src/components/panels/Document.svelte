<script lang="ts">
	import { getContext, onMount, tick } from "svelte";

	import type { DocumentState } from "@graphite/state-providers/document";
	import { textInputCleanup } from "@graphite/utility-functions/keyboard-entry";
	import { extractPixelData, rasterizeSVGCanvas } from "@graphite/utility-functions/rasterization";
	import type { Editor } from "@graphite/wasm-communication/editor";
	import {
		type MouseCursorIcon,
		type XY,
		DisplayEditableTextbox,
		DisplayEditableTextboxTransform,
		DisplayRemoveEditableTextbox,
		TriggerTextCommit,
		TriggerViewportResize,
		UpdateDocumentArtboards,
		UpdateDocumentArtwork,
		UpdateDocumentOverlays,
		UpdateDocumentRulers,
		UpdateDocumentScrollbars,
		UpdateEyedropperSamplingState,
		UpdateMouseCursor,
		UpdateDocumentNodeRender,
	} from "@graphite/wasm-communication/messages";

	import EyedropperPreview, { ZOOM_WINDOW_DIMENSIONS } from "@graphite/components/floating-menus/EyedropperPreview.svelte";
	import LayoutCol from "@graphite/components/layout/LayoutCol.svelte";
	import LayoutRow from "@graphite/components/layout/LayoutRow.svelte";
	import Graph from "@graphite/components/views/Graph.svelte";
	import RulerInput from "@graphite/components/widgets/inputs/RulerInput.svelte";
	import ScrollbarInput from "@graphite/components/widgets/inputs/ScrollbarInput.svelte";
	import WidgetLayout from "@graphite/components/widgets/WidgetLayout.svelte";

	let rulerHorizontal: RulerInput | undefined;
	let rulerVertical: RulerInput | undefined;
	let viewport: HTMLDivElement | undefined;

	const editor = getContext<Editor>("editor");
	const document = getContext<DocumentState>("document");

	// Interactive text editing
	let textInput: undefined | HTMLDivElement = undefined;
	let showTextInput: boolean;
	let textInputMatrix: number[];

	// CSS properties
	let canvasSvgWidth: number | undefined = undefined;
	let canvasSvgHeight: number | undefined = undefined;
	let canvasCursor = "default";

	// Scrollbars
	let scrollbarPos: XY = { x: 0.5, y: 0.5 };
	let scrollbarSize: XY = { x: 0.5, y: 0.5 };
	let scrollbarMultiplier: XY = { x: 0, y: 0 };

	// Rulers
	let rulerOrigin: XY = { x: 0, y: 0 };
	let rulerSpacing = 100;
	let rulerInterval = 100;
	let rulersVisible = true;

	// Rendered SVG viewport data
	let artworkSvg = "";
	let nodeRenderSvg = "";
	let artboardSvg = "";
	let overlaysSvg = "";

	// Rasterized SVG viewport data, or none if it's not up-to-date
	let rasterizedCanvas: HTMLCanvasElement | undefined = undefined;
	let rasterizedContext: CanvasRenderingContext2D | undefined = undefined;

	// Cursor position for cursor floating menus like the Eyedropper tool zoom
	let cursorLeft = 0;
	let cursorTop = 0;
	let cursorEyedropper = false;
	let cursorEyedropperPreviewImageData: ImageData | undefined = undefined;
	let cursorEyedropperPreviewColorChoice = "";
	let cursorEyedropperPreviewColorPrimary = "";
	let cursorEyedropperPreviewColorSecondary = "";

	$: canvasWidthCSS = canvasDimensionCSS(canvasSvgWidth);
	$: canvasHeightCSS = canvasDimensionCSS(canvasSvgHeight);

	function pasteFile(e: DragEvent) {
		const { dataTransfer } = e;
		if (!dataTransfer) return;
		e.preventDefault();

		Array.from(dataTransfer.items).forEach(async (item) => {
			const file = item.getAsFile();
			if (file?.type.startsWith("image")) {
				const imageData = await extractPixelData(file);

				editor.instance.pasteImage(new Uint8Array(imageData.data), imageData.width, imageData.height, e.clientX, e.clientY);
			}
		});
	}

	function translateCanvasX(newValue: number) {
		const delta = newValue - scrollbarPos.x;
		scrollbarPos.x = newValue;
		editor.instance.translateCanvas(-delta * scrollbarMultiplier.x, 0);
	}

	function translateCanvasY(newValue: number) {
		const delta = newValue - scrollbarPos.y;
		scrollbarPos.y = newValue;
		editor.instance.translateCanvas(0, -delta * scrollbarMultiplier.y);
	}

	function pageX(delta: number) {
		const move = delta < 0 ? 1 : -1;
		editor.instance.translateCanvasByFraction(move, 0);
	}

	function pageY(delta: number) {
		const move = delta < 0 ? 1 : -1;
		editor.instance.translateCanvasByFraction(0, move);
	}

	function canvasPointerDown(e: PointerEvent) {
		const onEditbox = e.target instanceof HTMLDivElement && e.target.contentEditable;

		if (!onEditbox) viewport?.setPointerCapture(e.pointerId);
	}

	// Update rendered SVGs
	export async function updateDocumentArtwork(svg: string) {
		artworkSvg = svg;
		rasterizedCanvas = undefined;

		await tick();

		const placeholders = window.document.querySelectorAll("[data-viewport] [data-canvas-placeholder]");
		// Replace the placeholders with the actual canvas elements
		placeholders.forEach((placeholder) => {
			const canvasName = placeholder.getAttribute("data-canvas-placeholder");
			if (!canvasName) return;
			// Get the canvas element from the global storage
			// eslint-disable-next-line @typescript-eslint/no-explicit-any
			const canvas = (window as any).imageCanvases[canvasName];
			placeholder.replaceWith(canvas);
		});
	}

	export function updateDocumentOverlays(svg: string) {
		overlaysSvg = svg;
	}

	export function updateDocumentArtboards(svg: string) {
		artboardSvg = svg;
		rasterizedCanvas = undefined;
	}

	export function updateDocumentNodeRender(svg: string) {
		nodeRenderSvg = svg;
		rasterizedCanvas = undefined;
	}

	export async function updateEyedropperSamplingState(mousePosition: XY | undefined, colorPrimary: string, colorSecondary: string): Promise<[number, number, number] | undefined> {
		if (mousePosition === undefined) {
			cursorEyedropper = false;
			return undefined;
		}
		cursorEyedropper = true;

		if (canvasSvgWidth === undefined || canvasSvgHeight === undefined) return undefined;

		cursorLeft = mousePosition.x;
		cursorTop = mousePosition.y;

		// This works nearly perfectly, but sometimes at odd DPI scale factors like 1.25, the anti-aliasing color can yield slightly incorrect colors (potential room for future improvement)
		const dpiFactor = window.devicePixelRatio;
		const [width, height] = [canvasSvgWidth, canvasSvgHeight];

		const outsideArtboardsColor = getComputedStyle(window.document.documentElement).getPropertyValue("--color-2-mildblack");
		const outsideArtboards = `<rect x="0" y="0" width="100%" height="100%" fill="${outsideArtboardsColor}" />`;

		const svg = `
			<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}">${outsideArtboards}${artboardSvg}${nodeRenderSvg}</svg>
			`.trim();

		if (!rasterizedCanvas) {
			rasterizedCanvas = await rasterizeSVGCanvas(svg, width * dpiFactor, height * dpiFactor, "image/png");
			rasterizedContext = rasterizedCanvas.getContext("2d") || undefined;
		}
		if (!rasterizedContext) return undefined;

		const rgbToHex = (r: number, g: number, b: number): string => `#${[r, g, b].map((x) => x.toString(16).padStart(2, "0")).join("")}`;

		const pixel = rasterizedContext.getImageData(mousePosition.x * dpiFactor, mousePosition.y * dpiFactor, 1, 1).data;
		const hex = rgbToHex(pixel[0], pixel[1], pixel[2]);
		const rgb: [number, number, number] = [pixel[0] / 255, pixel[1] / 255, pixel[2] / 255];

		cursorEyedropperPreviewColorChoice = hex;
		cursorEyedropperPreviewColorPrimary = colorPrimary;
		cursorEyedropperPreviewColorSecondary = colorSecondary;

		const previewRegion = rasterizedContext.getImageData(
			mousePosition.x * dpiFactor - (ZOOM_WINDOW_DIMENSIONS - 1) / 2,
			mousePosition.y * dpiFactor - (ZOOM_WINDOW_DIMENSIONS - 1) / 2,
			ZOOM_WINDOW_DIMENSIONS,
			ZOOM_WINDOW_DIMENSIONS,
		);
		cursorEyedropperPreviewImageData = previewRegion;

		return rgb;
	}

	// Update scrollbars and rulers
	export function updateDocumentScrollbars(position: XY, size: XY, multiplier: XY) {
		scrollbarPos = position;
		scrollbarSize = size;
		scrollbarMultiplier = multiplier;
	}

	export function updateDocumentRulers(origin: XY, spacing: number, interval: number, visible: boolean) {
		rulerOrigin = origin;
		rulerSpacing = spacing;
		rulerInterval = interval;
		rulersVisible = visible;
	}

	// Update mouse cursor icon
	export function updateMouseCursor(cursor: MouseCursorIcon) {
		let cursorString: string = cursor;

		// This isn't very clean but it's good enough for now until we need more icons, then we can build something more robust (consider blob URLs)
		if (cursor === "custom-rotate") {
			const svg = `
					<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="20" height="20">
						<path transform="translate(2 2)" fill="black" stroke="black" stroke-width="2px" d="
						M8,15.2C4,15.2,0.8,12,0.8,8C0.8,4,4,0.8,8,0.8c2,0,3.9,0.8,5.3,2.3l-1,1C11.2,2.9,9.6,2.2,8,2.2C4.8,2.2,2.2,4.8,2.2,8s2.6,5.8,5.8,5.8s5.8-2.6,5.8-5.8h1.4C15.2,12,12,15.2,8,15.2z
						" />
						<polygon transform="translate(2 2)" fill="black" stroke="black" stroke-width="2px" points="12.6,0 15.5,5 9.7,5" />
						<path transform="translate(2 2)" fill="white" d="
						M8,15.2C4,15.2,0.8,12,0.8,8C0.8,4,4,0.8,8,0.8c2,0,3.9,0.8,5.3,2.3l-1,1C11.2,2.9,9.6,2.2,8,2.2C4.8,2.2,2.2,4.8,2.2,8s2.6,5.8,5.8,5.8s5.8-2.6,5.8-5.8h1.4C15.2,12,12,15.2,8,15.2z
						" />
						<polygon transform="translate(2 2)" fill="white" points="12.6,0 15.5,5 9.7,5" />
					</svg>
					`
				.split("\n")
				.map((line) => line.trim())
				.join("");

			cursorString = `url('data:image/svg+xml;utf8,${svg}') 8 8, alias`;
		}

		canvasCursor = cursorString;
	}

	// Text entry
	export function triggerTextCommit() {
		if (!textInput) return;
		const textCleaned = textInputCleanup(textInput.innerText);
		editor.instance.onChangeText(textCleaned);
	}

	export async function displayEditableTextbox(displayEditableTextbox: DisplayEditableTextbox) {
		showTextInput = true;

		await tick();

		if (!textInput) {
			return;
		}

		if (displayEditableTextbox.text === "") textInput.textContent = "";
		else textInput.textContent = `${displayEditableTextbox.text}\n`;

		textInput.contentEditable = "true";
		textInput.style.transformOrigin = "0 0";
		textInput.style.width = displayEditableTextbox.lineWidth ? `${displayEditableTextbox.lineWidth}px` : "max-content";
		textInput.style.height = "auto";
		textInput.style.fontSize = `${displayEditableTextbox.fontSize}px`;
		textInput.style.color = displayEditableTextbox.color.toHexOptionalAlpha() || "transparent";

		textInput.oninput = () => {
			if (!textInput) return;
			editor.instance.updateBounds(textInputCleanup(textInput.innerText));
		};
		textInputMatrix = displayEditableTextbox.transform;
		const newFont = new FontFace("text-font", `url(${displayEditableTextbox.url})`);
		window.document.fonts.add(newFont);
		textInput.style.fontFamily = "text-font";

		// Necessary to select contenteditable: https://stackoverflow.com/questions/6139107/programmatically-select-text-in-a-contenteditable-html-element/6150060#6150060

		const range = window.document.createRange();
		range.selectNodeContents(textInput);

		const selection = window.getSelection();
		if (selection) {
			selection.removeAllRanges();
			selection.addRange(range);
		}

		textInput.focus();
		textInput.click();

		// Sends the text input element used for interactively editing with the text tool in a custom event
		window.dispatchEvent(new CustomEvent("modifyinputfield", { detail: textInput }));
	}

	export function displayRemoveEditableTextbox() {
		window.dispatchEvent(new CustomEvent("modifyinputfield", { detail: undefined }));
		showTextInput = false;
	}

	// Resize elements to render the new viewport size
	export function viewportResize() {
		if (!viewport) return;

		// Resize the canvas
		canvasSvgWidth = Math.ceil(parseFloat(getComputedStyle(viewport).width));
		canvasSvgHeight = Math.ceil(parseFloat(getComputedStyle(viewport).height));

		// Resize the rulers
		rulerHorizontal?.resize();
		rulerVertical?.resize();
	}

	function canvasDimensionCSS(dimension: number | undefined): string {
		// Temporary placeholder until the first actual value is populated
		// This at least gets close to the correct value but an actual number is required to prevent CSS from causing non-integer sizing making the SVG render with anti-aliasing
		if (dimension === undefined) return "100%";

		// Dimension is rounded up to the nearest even number because resizing is centered, and dividing an odd number by 2 for centering causes antialiasing
		return `${dimension % 2 === 1 ? dimension + 1 : dimension}px`;
	}

	onMount(() => {
		// Update rendered SVGs
		editor.subscriptions.subscribeJsMessage(UpdateDocumentArtwork, async (data) => {
			await tick();

			updateDocumentArtwork(data.svg);
		});
		editor.subscriptions.subscribeJsMessage(UpdateDocumentOverlays, async (data) => {
			await tick();

			updateDocumentOverlays(data.svg);
		});
		editor.subscriptions.subscribeJsMessage(UpdateDocumentArtboards, async (data) => {
			await tick();

			updateDocumentArtboards(data.svg);
		});
		editor.subscriptions.subscribeJsMessage(UpdateDocumentNodeRender, async (data) => {
			await tick();

			updateDocumentNodeRender(data.svg);
		});
		editor.subscriptions.subscribeJsMessage(UpdateEyedropperSamplingState, async (data) => {
			await tick();

			const { mousePosition, primaryColor, secondaryColor, setColorChoice } = data;
			const rgb = await updateEyedropperSamplingState(mousePosition, primaryColor, secondaryColor);

			if (setColorChoice && rgb) {
				if (setColorChoice === "Primary") editor.instance.updatePrimaryColor(...rgb, 1);
				if (setColorChoice === "Secondary") editor.instance.updateSecondaryColor(...rgb, 1);
			}
		});

		// Update scrollbars and rulers
		editor.subscriptions.subscribeJsMessage(UpdateDocumentScrollbars, async (data) => {
			await tick();

			const { position, size, multiplier } = data;
			updateDocumentScrollbars(position, size, multiplier);
		});
		editor.subscriptions.subscribeJsMessage(UpdateDocumentRulers, async (data) => {
			await tick();

			const { origin, spacing, interval, visible } = data;
			updateDocumentRulers(origin, spacing, interval, visible);
		});

		// Update mouse cursor icon
		editor.subscriptions.subscribeJsMessage(UpdateMouseCursor, async (data) => {
			await tick();

			const { cursor } = data;
			updateMouseCursor(cursor);
		});

		// Text entry
		editor.subscriptions.subscribeJsMessage(TriggerTextCommit, async () => {
			await tick();

			triggerTextCommit();
		});
		editor.subscriptions.subscribeJsMessage(DisplayEditableTextbox, async (data) => {
			await tick();

			displayEditableTextbox(data);
		});
		editor.subscriptions.subscribeJsMessage(DisplayEditableTextboxTransform, async (data) => {
			textInputMatrix = data.transform;
		});
		editor.subscriptions.subscribeJsMessage(DisplayRemoveEditableTextbox, async () => {
			await tick();

			displayRemoveEditableTextbox();
		});

		// Resize elements to render the new viewport size
		editor.subscriptions.subscribeJsMessage(TriggerViewportResize, async () => {
			await tick();

			viewportResize();
		});

		// Once this component is mounted, we want to resend the document bounds to the backend via the resize event handler which does that
		window.dispatchEvent(new Event("resize"));
	});
</script>

<LayoutCol class="document">
	<LayoutRow class="options-bar" classes={{ "for-graph": $document.graphViewOverlayOpen }} scrollableX={true}>
		{#if !$document.graphViewOverlayOpen}
			<WidgetLayout layout={$document.documentModeLayout} />
			<WidgetLayout layout={$document.toolOptionsLayout} />
			<LayoutRow class="spacer" />
			<WidgetLayout layout={$document.documentBarLayout} />
		{:else}
			<WidgetLayout layout={$document.nodeGraphBarLayout} />
		{/if}
	</LayoutRow>
	<LayoutRow class="shelf-and-table">
		<LayoutCol class="shelf">
			{#if !$document.graphViewOverlayOpen}
				<LayoutCol class="tools" scrollableY={true}>
					<WidgetLayout layout={$document.toolShelfLayout} />
				</LayoutCol>
			{/if}
			<LayoutCol class="spacer" />
			<LayoutCol class="shelf-bottom-widgets">
				<WidgetLayout layout={$document.graphViewOverlayButtonLayout} />
				<WidgetLayout layout={$document.workingColorsLayout} />
			</LayoutCol>
		</LayoutCol>
		<LayoutCol class="table">
			{#if rulersVisible}
				<LayoutRow class="ruler-or-scrollbar top-ruler">
					<RulerInput origin={rulerOrigin.x} majorMarkSpacing={rulerSpacing} numberInterval={rulerInterval} direction="Horizontal" bind:this={rulerHorizontal} />
				</LayoutRow>
			{/if}
			<LayoutRow class="viewport-container">
				{#if rulersVisible}
					<LayoutCol class="ruler-or-scrollbar">
						<RulerInput origin={rulerOrigin.y} majorMarkSpacing={rulerSpacing} numberInterval={rulerInterval} direction="Vertical" bind:this={rulerVertical} />
					</LayoutCol>
				{/if}
				<LayoutCol class="viewport-container" styles={{ cursor: canvasCursor }}>
					{#if cursorEyedropper}
						<EyedropperPreview
							colorChoice={cursorEyedropperPreviewColorChoice}
							primaryColor={cursorEyedropperPreviewColorPrimary}
							secondaryColor={cursorEyedropperPreviewColorSecondary}
							imageData={cursorEyedropperPreviewImageData}
							x={cursorLeft}
							y={cursorTop}
						/>
					{/if}
					<div class="viewport" on:pointerdown={(e) => canvasPointerDown(e)} on:dragover={(e) => e.preventDefault()} on:drop={(e) => pasteFile(e)} bind:this={viewport} data-viewport>
						<svg class="artboards" style:width={canvasWidthCSS} style:height={canvasHeightCSS}>
							{@html artboardSvg}
						</svg>
						<svg class="artboards" style:width={canvasWidthCSS} style:height={canvasHeightCSS}>
							{@html nodeRenderSvg}
						</svg>
						<svg class="artwork" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" style:width={canvasWidthCSS} style:height={canvasHeightCSS}>
							{@html artworkSvg}
						</svg>
						<svg class="overlays" style:width={canvasWidthCSS} style:height={canvasHeightCSS}>
							{@html overlaysSvg}
						</svg>
						<div class="text-input" style:width={canvasWidthCSS} style:height={canvasHeightCSS}>
							{#if showTextInput}
								<div bind:this={textInput} style:transform="matrix({textInputMatrix})" />
							{/if}
						</div>
					</div>
					<div class="graph-view" class:open={$document.graphViewOverlayOpen} style:--fade-artwork="80%" data-graph>
						<Graph />
					</div>
				</LayoutCol>
				<LayoutCol class="ruler-or-scrollbar right-scrollbar">
					<ScrollbarInput
						direction="Vertical"
						handleLength={scrollbarSize.y}
						handlePosition={scrollbarPos.y}
						on:handlePosition={({ detail }) => translateCanvasY(detail)}
						on:pressTrack={({ detail }) => pageY(detail)}
					/>
				</LayoutCol>
			</LayoutRow>
			<LayoutRow class="ruler-or-scrollbar bottom-scrollbar">
				<ScrollbarInput
					direction="Horizontal"
					handleLength={scrollbarSize.x}
					handlePosition={scrollbarPos.x}
					on:handlePosition={({ detail }) => translateCanvasX(detail)}
					on:pressTrack={({ detail }) => pageX(detail)}
				/>
			</LayoutRow>
		</LayoutCol>
	</LayoutRow>
</LayoutCol>

<style lang="scss" global>
	.document {
		height: 100%;

		.options-bar {
			height: 32px;
			flex: 0 0 auto;
			margin: 0 4px;

			.spacer {
				min-width: 40px;
			}

			&.for-graph .widget-layout {
				flex-direction: row;
				flex-grow: 1;
				justify-content: space-between;
			}
		}

		.shelf-and-table {
			.shelf {
				width: 32px;
				flex: 0 0 auto;

				.tools {
					flex: 0 1 auto;

					.icon-button[title^="Coming Soon"] {
						opacity: 0.25;
						transition: opacity 0.25s;

						&:hover {
							opacity: 1;
						}
					}

					.icon-button:not(.active) {
						.color-general {
							fill: var(--color-data-general);
						}

						.color-vector {
							fill: var(--color-data-vector);
						}

						.color-raster {
							fill: var(--color-data-raster);
						}
					}
				}

				.spacer {
					flex: 1 0 auto;
					min-height: 20px;
				}

				.shelf-bottom-widgets {
					flex: 0 0 auto;

					.widget-layout:first-of-type {
						height: auto;
						align-items: center;
					}

					.widget-layout:last-of-type {
						height: auto;

						.widget-span.row {
							min-height: 0;

							.working-colors-button {
								margin: 0;
							}

							.icon-button {
								--widget-height: 0;
							}
						}
					}
				}
			}

			.table {
				flex: 1 1 100%;

				.ruler-or-scrollbar {
					flex: 0 0 auto;
				}

				.top-ruler .ruler-input {
					padding-left: 16px;
					margin-right: 16px;
				}

				.right-scrollbar .scrollbar-input {
					margin-top: -16px;
				}

				.bottom-scrollbar .scrollbar-input {
					margin-right: 16px;
				}

				.viewport-container {
					flex: 1 1 100%;
					position: relative;

					.viewport {
						background: var(--color-2-mildblack);
						width: 100%;
						height: 100%;
						// Allows the SVG to be placed at explicit integer values of width and height to prevent non-pixel-perfect SVG scaling
						position: relative;
						overflow: hidden;

						svg {
							position: absolute;
							// Fallback values if JS hasn't set these to integers yet
							width: 100%;
							height: 100%;
							// Allows dev tools to select the artwork without being blocked by the SVG containers
							pointer-events: none;

							canvas {
								width: 100%;
								height: 100%;
							}

							// Prevent inheritance from reaching the child elements
							> * {
								pointer-events: auto;
							}
						}

						.text-input div {
							cursor: text;
							background: none;
							border: none;
							margin: 0;
							padding: 0;
							overflow: visible;
							white-space: pre-wrap;
							display: inline-block;
							// Workaround to force Chrome to display the flashing text entry cursor when text is empty
							padding-left: 1px;
							margin-left: -1px;

							&:focus {
								border: none;
								outline: none; // Ok for contenteditable element
								margin: -1px;
							}
						}
					}

					.graph-view {
						pointer-events: none;
						transition: opacity 0.1s ease-in-out;
						opacity: 0;

						&.open {
							cursor: auto;
							pointer-events: auto;
							opacity: 1;
						}

						&::before {
							content: "";
							position: absolute;
							top: 0;
							left: 0;
							width: 100%;
							height: 100%;
							background: var(--color-2-mildblack);
							opacity: var(--fade-artwork);
							pointer-events: none;
						}
					}

					.fade-artwork,
					.graph {
						position: absolute;
						top: 0;
						left: 0;
						width: 100%;
						height: 100%;
					}
				}
			}
		}
	}
</style>
