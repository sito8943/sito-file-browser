// App id in the usage service's `APPS` list (sito-feature-usage).
export const USAGE_APP = "file-browser";

// Build-time endpoint and public app key. An empty endpoint turns tracking off (local builds
// without a .env, forks), so the app never depends on the service.
export const USAGE_URL = import.meta.env.VITE_USAGE_URL ?? "";
export const USAGE_KEY = import.meta.env.VITE_USAGE_KEY ?? "";

// Where the user's choice lives in the webview's local storage. The choice is per device and not
// part of settings.toml: it decides whether the webview may talk to the usage service at all.
export const CONSENT_STORAGE_KEY = "sfb:consent";

// Bump when the notice text or what is counted changes: older choices are asked again.
export const CONSENT_VERSION = 1;

// Feature-name prefixes. Every counted feature is `<prefix>.<snake_case id>`. The dotted ones
// (`sidebar.menu`, …) prefix the ids of a menu registry, so its catalogue entries follow the
// registry the same way `action.*` follows ENTRY_ACTION.
export const USAGE_PREFIX = {
  PAGE: "page",
  ACTION: "action",
  HOTKEY: "hotkey",
  SETTING: "setting",
  SIDEBAR_MENU: "sidebar.menu",
  VOLUMES_MENU: "volumes.menu",
  SETTINGS_SECTION: "settings.section",
} as const;

// Features that no registry describes, grouped by area and counted at their choke point (the one
// function every trigger — button, hotkey, menu — goes through). They overlap `hotkey.*` and
// `action.*` on purpose: those say which input was used, these say the feature was used.
export const USAGE_EVENT = {
  BROWSE: {
    // A real folder is shown (double-click, Enter, sidebar, path bar, Back/Forward).
    FOLDER: "browse.folder",
    // Virtual and remote listings, counted instead of FOLDER.
    RECENTS: "browse.recents",
    TAGS: "browse.tags",
    TRASH: "browse.trash",
    SFTP: "browse.sftp",
    // A file is opened in its default OS app.
    OPEN_FILE: "browse.open_file",
  },
  PREVIEW: {
    // The in-app preview opened, by the kind of file it shows.
    IMAGE: "preview.image",
    VIDEO: "preview.video",
    AUDIO: "preview.audio",
    PDF: "preview.pdf",
    MARKDOWN: "preview.markdown",
    // The preview opened in its own detached window instead of the overlay.
    WINDOW: "preview.window",
    PREV: "preview.prev",
    NEXT: "preview.next",
    MAXIMIZE: "preview.maximize",
    IMAGE_ZOOM: "preview.image.zoom",
    IMAGE_ROTATE: "preview.image.rotate",
    IMAGE_COPY: "preview.image.copy",
    MARKDOWN_EDIT: "preview.markdown.edit",
    MARKDOWN_SAVE: "preview.markdown.save",
    MARKDOWN_FIND: "preview.markdown.find",
    AUDIO_PLAY: "preview.audio.play",
  },
  PROPERTIES: {
    DIALOG: "properties.dialog",
    WINDOW: "properties.window",
    SIDEBAR_VOLUME: "properties.sidebar_volume",
  },
  PANEL: {
    INFO_PANEL: "info_panel.toggle",
    STATUS_STORAGE: "status.storage",
    STATUS_SYSTEM_MONITOR: "status.system_monitor",
  },
  TABS: {
    NEW: "tabs.new",
    CLOSE: "tabs.close",
    SWITCH: "tabs.switch",
    REORDER: "tabs.reorder",
  },
  NAV: {
    BACK: "nav.back",
    FORWARD: "nav.forward",
    UP: "nav.up",
    HOME: "nav.home",
    PATH_TYPED: "nav.path_typed",
    CRUMB: "nav.crumb",
  },
  SEARCH: {
    RUN: "search.run",
    RECENT_PICK: "search.recent_pick",
    RECENT_CLEAR: "search.recent_clear",
    FILTER_KIND: "search.filter.kind",
    FILTER_DATE: "search.filter.date",
    FILTER_SIZE: "search.filter.size",
    FILTER_RESET: "search.filter.reset",
  },
  VIEW: {
    SORT: "sort.change",
    COLUMNS: "columns.toggle",
    GRID: "view.grid",
    LIST: "view.list",
    ZOOM: "zoom.directory",
  },
  SIDEBAR: {
    COLLAPSE: "sidebar.collapse",
    RESIZE: "sidebar.resize",
    EDIT_MODE: "sidebar.edit_mode",
    SECTION_TOGGLE: "sidebar.section_toggle",
    ITEM_ADD: "sidebar.item.add",
    ITEM_REMOVE: "sidebar.item.remove",
    PRESET_HIDE: "sidebar.preset.hide",
    GROUP_ADD: "sidebar.group.add",
    GROUP_DELETE: "sidebar.group.delete",
    GROUP_RENAME: "sidebar.group.rename",
    GROUP_REORDER: "sidebar.group.reorder",
    EJECT_BUTTON: "sidebar.eject_button",
  },
  FILES: {
    TAGS_TOGGLE: "tags.toggle",
    // Drop of entries dragged inside the app vs files dragged in from another app.
    DROP: "dnd.drop",
    IMPORT: "dnd.import",
    DRAG_OUT: "dnd.drag_out",
    RENAME: "rename.commit",
    MARQUEE: "select.marquee",
    TYPEAHEAD: "select.typeahead",
    COMPRESS: "archive.compress",
    EXTRACT_PASSWORD: "archive.extract_password",
    CUSTOM_ACTION_RUN: "custom_action.run",
  },
  CONNECTIONS: {
    SFTP_CONNECT: "sftp.connect",
    SFTP_ADD: "sftp.add",
    SFTP_EDIT: "sftp.edit",
    SFTP_REMOVE: "sftp.remove",
    SFTP_REAUTH: "sftp.reauth",
    SMB_OPEN: "smb.open",
    SMB_ADD: "smb.add",
    SMB_EDIT: "smb.edit",
    SMB_REMOVE: "smb.remove",
  },
  REMOTE: {
    SIGN_IN: "remote.sign_in",
    SIGN_OUT: "remote.sign_out",
    ROOT_ADD: "remote.root.add",
    ROOT_REMOVE: "remote.root.remove",
    PAIR_OPEN: "remote.pair.open",
    PAIR_CANCEL: "remote.pair.cancel",
    PAIR_CONFIRM: "remote.pair.confirm",
    DEVICE_REVOKE: "remote.device.revoke",
  },
  APP: {
    SETTINGS_OPEN: "settings.open",
    SETTINGS_IMPORT: "settings.import",
    SETTINGS_EXPORT: "settings.export",
    CLEANUP_ADD: "cleanup.add",
    CLEANUP_REMOVE: "cleanup.remove",
    CLEANUP_MODE: "cleanup.mode",
    CLEANUP_CLEAN: "cleanup.clean",
    STORAGE_CLEAR_CACHE: "storage.clear_cache",
    CUSTOM_ACTION_SAVE: "custom_action.save",
    CUSTOM_ACTION_IMPORT: "custom_action.import",
    CUSTOM_ACTION_EXPORT: "custom_action.export",
    THUMBNAIL_PROBE: "thumbnail_probe.test",
    FOLDER_HANDLER: "folder_handler.toggle",
    UPDATE_CHECK: "update.check_manual",
    UPDATE_TOAST: "update.toast_click",
    ONBOARDING_OPEN: "onboarding.open",
    ONBOARDING_SKIP: "onboarding.skip",
    ONBOARDING_FINISH: "onboarding.finish",
    CHANGELOG_OPEN: "changelog.open",
    PICKER_FOLDER: "picker.folder",
    PICKER_FILE: "picker.file",
    NOTICE_FULL_DISK_ACCESS: "notice.full_disk_access",
    NOTICE_STALLED_LEAVE: "notice.stalled_leave",
    NOTICE_NTFS: "notice.ntfs",
    NOTICE_REMOTE_RETRY: "notice.remote_retry",
    DOCK_RECENT_FOLDER: "dock.recent_folder",
    DOCK_ACTION: "dock.action",
    // The window opened on a path from Finder Sync, the URL scheme or Open With.
    WINDOW_FROM_URL: "window.from_url",
  },
} as const;

// Calls closer together than this belong to one burst (one wheel zoom, one typed prefix) and
// count as a single use. See useTrackBurst.
export const USAGE_BURST_GAP_MS = 1000;

export const DATA_CHOICES_TITLE_ID = "data_choices_title";
