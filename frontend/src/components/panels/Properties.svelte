<script lang="ts">
	import { getContext, onMount } from "svelte";

	import type { Editor } from "@graphite/wasm-communication/editor";
	import { defaultWidgetLayout, patchWidgetLayout, UpdatePropertyPanelOptionsLayout, UpdatePropertyPanelSectionsLayout } from "@graphite/wasm-communication/messages";

	import LayoutCol from "@graphite/components/layout/LayoutCol.svelte";
	import LayoutRow from "@graphite/components/layout/LayoutRow.svelte";
	import WidgetLayout from "@graphite/components/widgets/WidgetLayout.svelte";

	const editor = getContext<Editor>("editor");

	let propertiesOptionsLayout = defaultWidgetLayout();
	let propertiesSectionsLayout = defaultWidgetLayout();

	onMount(() => {
		editor.subscriptions.subscribeJsMessage(UpdatePropertyPanelOptionsLayout, (updatePropertyPanelOptionsLayout) => {
			patchWidgetLayout(propertiesOptionsLayout, updatePropertyPanelOptionsLayout);
			propertiesOptionsLayout = propertiesOptionsLayout;
		});

		editor.subscriptions.subscribeJsMessage(UpdatePropertyPanelSectionsLayout, (updatePropertyPanelSectionsLayout) => {
			patchWidgetLayout(propertiesSectionsLayout, updatePropertyPanelSectionsLayout);
			propertiesSectionsLayout = propertiesSectionsLayout;
		});
	});
</script>

<LayoutCol class="properties">
	<LayoutRow class="options-bar">
		<WidgetLayout layout={propertiesOptionsLayout} />
	</LayoutRow>
	<LayoutRow class="sections" scrollableY={true}>
		<WidgetLayout layout={propertiesSectionsLayout} />
	</LayoutRow>
</LayoutCol>

<style lang="scss" global>
	.properties {
		height: 100%;

		.widget-layout {
			flex: 1 1 100%;
			margin: 0 4px;
		}

		.options-bar {
			height: 32px;
			flex: 0 0 auto;
		}

		.sections {
			flex: 1 1 100%;
		}

		.text-button {
			flex-basis: 0;
		}
	}
</style>
