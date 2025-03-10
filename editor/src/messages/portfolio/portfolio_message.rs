use crate::messages::portfolio::document::utility_types::clipboards::Clipboard;
use crate::messages::prelude::*;

use document_legacy::document_metadata::LayerNodeIdentifier;
use document_legacy::LayerId;
use graph_craft::document::NodeId;
use graphene_core::text::Font;

use serde::{Deserialize, Serialize};

#[remain::sorted]
#[impl_message(Message, Portfolio)]
#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum PortfolioMessage {
	// Sub-messages
	#[remain::unsorted]
	#[child]
	MenuBar(MenuBarMessage),
	#[remain::unsorted]
	#[child]
	Document(DocumentMessage),

	// Messages
	#[remain::unsorted]
	DocumentPassMessage {
		document_id: u64,
		message: DocumentMessage,
	},
	AutoSaveActiveDocument,
	AutoSaveDocument {
		document_id: u64,
	},
	CloseActiveDocumentWithConfirmation,
	CloseAllDocuments,
	CloseAllDocumentsWithConfirmation,
	CloseDocument {
		document_id: u64,
	},
	CloseDocumentWithConfirmation {
		document_id: u64,
	},
	Copy {
		clipboard: Clipboard,
	},
	Cut {
		clipboard: Clipboard,
	},
	DeleteDocument {
		document_id: u64,
	},
	DestroyAllDocuments,
	FontLoaded {
		font_family: String,
		font_style: String,
		preview_url: String,
		data: Vec<u8>,
		is_default: bool,
	},
	GraphViewOverlay {
		open: bool,
	},
	GraphViewOverlayToggle,
	ImaginateCheckServerStatus,
	ImaginatePollServerStatus,
	ImaginatePreferences,
	ImaginateServerHostname,
	Import,
	LoadDocumentResources {
		document_id: u64,
	},
	LoadFont {
		font: Font,
		is_default: bool,
	},
	NewDocumentWithName {
		name: String,
	},
	NextDocument,
	OpenDocument,
	OpenDocumentFile {
		document_name: String,
		document_serialized_content: String,
	},
	OpenDocumentFileWithId {
		document_id: u64,
		document_name: String,
		document_is_auto_saved: bool,
		document_is_saved: bool,
		document_serialized_content: String,
	},
	PasteIntoFolder {
		clipboard: Clipboard,
		parent: LayerNodeIdentifier,
		insert_index: isize,
	},
	PasteSerializedData {
		data: String,
	},
	PrevDocument,
	SelectDocument {
		document_id: u64,
	},
	SetActiveDocument {
		document_id: u64,
	},
	SetImageBlobUrl {
		document_id: u64,
		layer_path: Vec<LayerId>,
		node_id: Option<NodeId>,
		blob_url: String,
		resolution: (f64, f64),
	},
	SubmitGraphRender {
		document_id: u64,
		layer_path: Vec<LayerId>,
	},
	ToggleRulers,
	UpdateDocumentWidgets,
	UpdateOpenDocumentsList,
}
