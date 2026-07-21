//! English catalog (RFC-031). Exhaustive over [`MessageKey`].

use super::MessageKey;

pub fn message(key: MessageKey) -> &'static str {
    use MessageKey::*;
    match key {
        AppTitle => "orbok",
        LocalOnlyBadge => "Local Only",
        NavSearch => "Search",
        NavSources => "Folders",
        NavIndexing => "Preparing",
        NavStorage => "Storage",
        NavModels => "Models",
        NavAi => "AI",
        NavSettings => "Settings",
        SearchPlaceholder => "Search local documents...",
        SearchButton => "Search",
        SearchNoSourcesTitle => "Nothing to search yet",
        SearchNoSourcesBody => "Add a folder or file so orbok can build a local search index.",
        SearchAddSource => "Add Source",
        SearchNoResults => "No results found",
        SearchKeywordOnlyNotice => "Search by meaning is not set up yet. Basic search still works.",
        SourcesTitle => "Folders",
        SourcesEmptyTitle => "No folders added",
        SourcesEmptyBody => {
            "Add folders or files that orbok is allowed to search. \
             orbok will not scan your entire computer automatically."
        }
        SourcesAddFolder => "Add Folder",
        SourcesStatusActive => "Active",
        SourcesStatusPaused => "Paused",
        SourcesStatusMissing => "Missing",
        IndexingTitle => "Preparing search",
        IndexingIdle => "Search is ready",
        IndexingHealthIndexed => "Indexed",
        IndexingHealthStale => "Stale",
        IndexingHealthFailed => "Failed",
        IndexingHealthQueued => "Queued",
        StorageTitle => "Storage",
        StorageIntro => "See what orbok stores and clean up safely.",
        StorageGroupSearchIndex => "Search data",
        StorageGroupModels => "Search helper",
        StorageGroupCaches => "Temporary previews",
        StorageSafeCleanupHeading => "Safe cleanup",
        StorageClearSnippets => "Clear temporary previews",
        StorageClearSearchCache => "Clear old search results",
        StorageDangerHeading => "Dangerous",
        StorageResetCatalog => "Reset saved app data...",
        StorageResetWarning => {
            "This removes registered sources and all indexes. \
             Your source files are never deleted."
        }
        ModelsTitle => "Models",
        ModelsEmbeddingRole => "Embedding",
        ModelsRerankerRole => "Reranker",
        ModelsStatusAvailable => "Available",
        ModelsStatusMissing => "Missing",
        ModelsKeywordOnlyHint => {
            "Basic search still works. Add a search helper to also \
             search by meaning."
        }
        ModelsVerification => "Verification",
        SettingsTitle => "Settings",
        SettingsLanguageHeading => "Language",
        SettingsPrivacyHeading => "Privacy",
        SettingsAdvancedHeading => "Advanced view",
        SettingsAdvancedOn => "Advanced view: On",
        SettingsAdvancedOff => "Advanced view: Off",
        SettingsAdvancedHint => "Show technical detail in search results, indexing, and storage.",
        SettingsPrivacyLocalOnly => "Documents are processed on this computer only.",
        SearchModeLabel => "Mode",
        SearchModeAuto => "Auto",
        SearchModeExact => "Exact",
        SearchModeConceptual => "Conceptual",
        SearchModeFast => "Fast",
        BadgeKeyword => "Keyword",
        BadgeSemantic => "Semantic",
        BadgeFused => "Fused",
        WizardTitleNotConfigured => "Set up search by meaning",
        WizardTitleFileMissing => "Embedding model not found",
        WizardTitleValidating => "Checking model folder",
        WizardTitleReady => "Embedding model ready",
        WizardBodyNotConfigured => {
            "Keyword search is ready. To also search by meaning,              orbok needs a local AI model on this computer.              No files are uploaded — inference runs locally."
        }
        WizardBodyFileMissing => {
            "The model folder is no longer at its expected location.              This can happen when a drive is disconnected or files are moved."
        }
        WizardFilesNeededLabel => "Required files in the folder:",
        WizardDownloadHint => "Download: huggingface-cli download intfloat/multilingual-e5-small",
        WizardPathInputPlaceholder => "Path to model folder (e.g. ~/models/multilingual-e5-small)",
        WizardActionLocate => "Locate model folder",
        WizardActionValidate => "Validate",
        WizardActionUseModel => "Use this model",
        WizardActionContinue => "Continue to orbok",
        WizardPathPlaceholder => "Folder path…",
        WizardDownloadAction => "Download from HuggingFace",
        WizardDownloadProgress => "Downloading model…",
        WizardActionSkip => "Skip — use keyword search only",
        WizardPreviousPathLabel => "Last known path",
        WizardValidationOk => "found",
        WizardValidationFail => "not found",
        WizardReadyBody => "Semantic search is now available.",
        ModelConsentTitle => "Review model download",
        ModelConsentBody => {
            "orbok will contact the provider and save this model locally only after you continue."
        }
        ModelConsentPrivacy => {
            "Your documents, searches, source paths, and this save location are not sent to the model provider."
        }
        ModelConsentProvider => "Provider",
        ModelConsentSource => "Source",
        ModelConsentRevision => "Immutable revision",
        ModelConsentExactSize => "Exact download size",
        ModelConsentLicense => "License",
        ModelConsentLocation => "Save location",
        ModelConsentVerification => "Verification",
        ModelTrustAppWillVerify => "orbok will verify the download before use",
        ModelTrustAppVerified => "App verified",
        ModelTrustUserSupplied => "User supplied / provenance not verified",
        ModelConsentConfirm => "Agree and download",
        ModelConsentCancel => "Back",
        NoticeDownloadFailTitle => "Download did not finish",
        NoticeDownloadFailBody => {
            "We could not finish the download. Please check your \
             connection and try again."
        }
        NoticeFolderFailTitle => "Folder was not added",
        NoticeFolderFailBody => {
            "We could not add that folder. Please choose another folder \
             or check that you can open it."
        }
        NoticeSearchFailTitle => "Search did not finish",
        NoticeSearchFailBody => "Something went wrong while searching. Please try again.",
        NoticeFilesMissingTitle => "Files may have moved",
        NoticeFilesMissingBody => {
            "Some files are no longer where orbok expected them. This can \
             happen if a drive was disconnected or files were moved."
        }
        NoticeFolderAddedTitle => "Folder added",
        NoticeFolderAddedBody => "orbok is preparing your search now.",
        NoticeSearchReadyTitle => "Search is ready",
        NoticeSearchReadyBody => "Your files are ready to search.",
        NoticePreviewsClearedTitle => "Temporary previews cleared",
        NoticePreviewsClearedBody => "Freed up space. Your files are untouched.",
        NoticeActionTryAgain => "Try again",
        NoticeActionChooseFolder => "Choose another folder",
        SettingsThemeHeading => "Theme",
        ThemeSystem => "Follow system",
        ThemeLight => "Light",
        ThemeDark => "Dark",
        ThemeHighContrastLight => "High contrast (light)",
        ThemeHighContrastDark => "High contrast (dark)",
        SettingsTextScaleHeading => "Text size",
        TextScaleDefault => "Default",
        TextScaleLarge => "Large",
        TextScaleLarger => "Larger",
        SettingsReduceMotion => "Reduce motion",
        SettingsReduceMotionHint => "Fewer animations and transitions.",
        SettingsCvdNote => {
            "Status colors are always shown with a label and an icon, so they stay clear for every kind of color vision."
        }
        NoticeSensitiveSourceTitle => "This folder may contain private files",
        NoticeSensitiveSourceBody => {
            "It may include SSH keys, browser profiles, or other sensitive data. The folder was added. Remove it if you did not intend to search it."
        }
        NoticeDismiss => "Dismiss",
        Cancel => "Cancel",
        Confirm => "Confirm",
        // RFC-041: Search, Narrow Results, Browse Around
        SearchNarrowResults => "Narrow results",
        SearchNarrowedBy => "Narrowed by",
        SearchMoreWays => "More ways to narrow",
        SearchClearFilters => "Clear",
        SearchNoResultsFiltered => "No results with these choices",
        SearchNoResultsFilteredBody => "Try removing one.",
        SearchInThisFolder => "Search in this folder",
        SearchShowNearby => "Show nearby files",
        SearchShowSimilar => "Show similar files",
        SearchResultsUpdating => "Updating results...",
        SearchPreparingFolder => "Preparing \"{folder}\" for search",
        SearchPartialReadiness => "{ready} files ready. You can search now.",
        // RFC-041 filter labels
        FilterKind => "Kind",
        FilterChanged => "Changed",
        FilterSearchIn => "Search in",
        FilterReadyStatus => "Ready status",
        FilterKindPdfs => "PDFs",
        FilterKindNotes => "Notes",
        FilterKindCode => "Code",
        FilterKindDocuments => "Documents",
        FilterKindSpreadsheets => "Spreadsheets",
        FilterChangedToday => "Today",
        FilterChangedThisWeek => "This week",
        FilterChangedThisMonth => "This month",
        FilterChangedAnyTime => "Any time",
        FilterAllFolders => "All folders",
        // RFC-037: Source lifecycle
        SourceStateReady => "Ready",
        SourceStatePreparing => "Preparing",
        SourceStateNeedsUpdate => "Needs update",
        SourceStatePaused => "Paused",
        SourceStateFolderNotFound => "Folder not found",
        SourceStateCannotOpen => "Cannot open",
        SourceStateRemoved => "Removed",
        SourceActionCheckAgain => "Check again",
        SourceActionPrepareAgain => "Prepare again",
        SourceActionChooseFolderAgain => "Choose folder again",
        SourceActionRemoveFromOrbok => "Remove from orbok",
        SourceFolderNotFoundDetail => {
            "This can happen if a drive is disconnected or the folder was moved."
        }
        SourceFilesNotDeletedNotice => {
            "Your files were not deleted. orbok just cannot find this folder right now."
        }
        SourceManyFilesChanged => "Many files changed. orbok will prepare them gradually.",
        SourcePausePreparation => "Pause preparing",
        SourceResumePreparation => "Resume preparing",
        // RFC-038: Result trust badges and recovery
        TrustNeedsUpdate => "Needs update",
        TrustFileNotFound => "File not found",
        TrustStillBeingPrepared => "Still being prepared",
        TrustPartlyPrepared => "Partly prepared",
        TrustCannotOpen => "Cannot open",
        TrustActionPrepareAgain => "Prepare again",
        TrustActionCheckFolder => "Check folder",
        TrustActionRemoveFromResults => "Remove from results",
        TrustActionOpenAnyway => "Open file anyway",
        TrustActionShowInFolder => "Show in folder",
        TrustActionViewDetails => "View details",
        TrustFileChangedDetail => "This file changed after orbok prepared it.",
        TrustFileNotFoundDetail => {
            "orbok cannot find this file. It may have been moved, deleted, or the drive may be disconnected."
        }
        TrustPartlyPreparedDetail => "Only part of this file was prepared.",
        TrustScannedPdfDetail => "This PDF may contain images instead of selectable text.",
        TrustSomePagesDetail => "Some pages could not be prepared.",
        TrustSizeLimitDetail => "Only part of this large file was prepared.",
        TrustCannotOpenDetail => "orbok cannot open this file.",
        // RFC-043: Model download readiness
        ModelCheckingFiles => "Checking search helper...",
        ModelAlreadyReady => "Better search is ready.",
        ModelNeedsDownload => {
            "Some search helper files are needed. orbok will download only what is missing."
        }
        ModelDownloadingBetterSearch => "Downloading better search",
        ModelFilesStayLocal => "Your files stay on this computer.",
        ModelDownloadFailed => {
            "Download did not finish. Please check your connection and try again."
        }
        ModelDownloadRetry => "Try again",
        ModelRepairingFiles => {
            "Some search helper files need to be repaired. orbok will download only what is needed."
        }
        ModelBasicSearchAvailable => "Basic search is ready. Search by meaning can be added later.",
        ModelDownloadingWhatNeeded => "Downloading what is needed...",
        // RFC-039: Privacy modes
        PrivacyTitle => "Privacy",
        PrivacyLocalOnlyStatement => "Documents are processed on this computer only.",
        PrivacyModeStandard => "Standard",
        PrivacyModeStrict => "Strict",
        PrivacyModePortable => "Portable",
        PrivacyModeStrictDescription => "Strict privacy reduces what orbok remembers.",
        PrivacyModePortableDescription => "orbok stores app data next to this copy of the app.",
        PrivacyRememberSearches => "Remember recent searches",
        PrivacyRememberSearchesHint => "Recent searches are saved on this computer only.",
        PrivacySearchesDisabledStrict => {
            "Recent searches are not saved while Strict privacy is on."
        }
        PrivacyTemporaryPreviews => "Temporary previews",
        PrivacyTemporaryPreviewsHint => {
            "Temporary previews help results open faster. You can clear them anytime."
        }
        PrivacyClearPreviews => "Clear temporary previews",
        PrivacyEnableStrictConfirm => "Turn on Strict privacy?",
        PrivacyEnableStrictBody => {
            "orbok will stop saving recent searches and reduce temporary previews. You can also clear data already saved."
        }
        PrivacyTurnOn => "Turn on",
        PrivacyTurnOnAndClear => "Turn on and clear",
        PrivacyFilesNotDeleted => "Your files will not be deleted.",
        PrivacyModelDownloadNote => {
            "orbok downloads the search helper, but your documents are not uploaded."
        }
        // RFC-040: Diagnostics
        DiagnosticsTitle => "Diagnostics",
        DiagnosticsIntro => {
            "Create a support file if something is not working. The file does not include your documents or search words by default."
        }
        DiagnosticsCreateFile => "Create support file",
        DiagnosticsPreviewTitle => "Create support file",
        DiagnosticsIncludedLabel => "Included",
        DiagnosticsExcludedLabel => "Not included",
        DiagnosticsOptInFolderNames => "Include folder names",
        DiagnosticsOptInFolderNamesHint => "This may reveal which folders you use.",
        DiagnosticsOptInSearchWords => "Include recent search words",
        DiagnosticsOptInSearchWordsHint => "This may reveal what you were looking for.",
        DiagnosticsFileCreated => "Support file created.",
        DiagnosticsShowFile => "Show file",
        DiagnosticsCreateFailed => {
            "Support file was not created. Please choose another location or try again."
        }
        // RFC-045: search-in-folder flow
        SearchInLabel => "Search in",
        SearchChooseFolder => "Choose a folder",
        SearchScopeOnly => "This folder only",
        SearchScopeSubfolders => "This folder and subfolders",
        SearchRecentFoldersLabel => "Recent folders",
        // RFC-042: search history
        RecentSearchesLabel => "Recent searches",
        SearchAgainButton => "Search again",
        SearchingAgainStatus => "Searching again\u{2026}",
        OpenRecentSearches => "Recent searches",
        ClearRecentSearches => "Clear recent searches",
        ClearRecentSearchesConfirmTitle => "Clear recent searches?",
        ClearRecentSearchesConfirmBody => {
            "This removes the list of searches shown in orbok. \
             Your files and search data are not deleted."
        }
        RecentSearchesClearedNotice => "Recent searches cleared.",
        RememberRecentSearches => "Remember recent searches",
        RecentSearchesPrivacyNote => "Recent searches are saved on this computer only.",
        RecentSearchesStrictPrivacyNote => {
            "Recent searches are not saved while Strict privacy is on."
        }
        NoRecentSearches => "No recent searches yet.",
        DroppedFilterNotice => "One narrowing choice was no longer available and was removed.",
    }
}
