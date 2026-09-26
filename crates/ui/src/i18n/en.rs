//! English catalog (RFC-031). Exhaustive over [`MessageKey`].

use super::MessageKey;

pub fn message(key: MessageKey) -> &'static str {
    use MessageKey::*;
    match key {
        AppTitle => "orbok",
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
        SearchNoSourcesBody => "Add a folder so orbok can search it.",
        SearchNoResults => "No results found",
        SearchKeywordOnlyNotice => {
            "Search by meaning is not set up yet. Keyword search still works."
        }
        SearchRunning => "Searching…",
        SearchSnippetUnavailable => "(preview unavailable)",
        SearchResultOpenFile => "Open file",
        SearchResultShowInFolder => "Show in folder",
        SourcesTitle => "Folders",
        SourcesEmptyTitle => "No folders added",
        SourcesEmptyBody => {
            "Add folders that orbok is allowed to search. \
             orbok will not scan your entire computer automatically."
        }
        SourcesAddFolder => "Add folder",
        SourcesPathInputPlaceholder => "Or type a path manually…",
        NarrowFolderTitle => "Stop including subfolders?",
        NarrowFolderBody => {
            "orbok removes what it prepared for files in this folder's subfolders. Your files are never changed or deleted."
        }
        NarrowFolderConfirm => "Stop including",
        IndexingTitle => "Preparing search",
        IndexingHealthIndexed => "Ready",
        IndexingHealthStale => "Needs update",
        IndexingHealthFailed => "Failed",
        IndexingHealthQueued => "Queued",
        IndexingHealthNoText => "No text",
        IndexingRunning => "Preparing…",
        StorageTitle => "Storage",
        StorageIntro => "See what orbok stores and clean up safely.",
        StorageGroupSearchIndex => "Search data",
        StorageGroupModels => "Models",
        StorageGroupCaches => "Temporary previews",
        StorageSafeCleanupHeading => "Safe cleanup",
        StorageClearSnippets => "Clear temporary previews",
        StorageClearSearchCache => "Clear old search results",
        StorageClearTemporaryExtraction => "Clear extracted text",
        StorageRemoveReplacedStaleIndexes => "Remove old data from updated files",
        StorageDangerHeading => "Dangerous",
        StorageResetCatalog => "Reset saved app data...",
        StorageResetWarning => {
            "This removes registered folders and all search data. \
             Your files are never changed or deleted."
        }
        // Task 091 (owner-approved 2026-09-23).
        StorageResetConfirmTitle => "Reset saved app data?",
        StorageResetConfirm => "Reset",
        // Task 081 (RFC-011 §13.1, owner-approved 2026-09-22).
        StorageNotCalculatedYet => "Storage usage has not been calculated yet.",
        StorageCalculateNow => "Calculate now",
        StorageValueUnknown => "Unknown",
        StorageCategoryPersistentCatalog => "Catalog",
        StorageCategoryKeywordIndex => "Keyword index",
        StorageCategoryVectorIndex => "Vector index",
        StorageCategorySnippetCache => "Temporary previews",
        StorageCategorySearchCache => "Old search results",
        StorageCategoryTemporaryExtraction => "Extracted text",
        StorageCategoryModelFiles => "Models",
        StorageCategoryLogs => "Logs",
        StorageCacheFileSize => "Cache file on disk",
        StorageRebuildKeywordButton => "Prepare keyword search again",
        StorageRebuildVectorButton => "Prepare search by meaning again",
        RebuildKeywordConfirmTitle => "Prepare keyword search again?",
        RebuildVectorConfirmTitle => "Prepare search by meaning again?",
        RebuildConfirmBody => {
            "orbok removes what it prepared and prepares it again. Your files are never \
             changed or deleted. Search may be incomplete until it finishes."
        }
        RebuildConfirm => "Prepare again",
        ModelsTitle => "Models",
        ModelsEmbeddingRole => "Search by meaning",
        ModelsRerankerRole => "Reranker",
        ModelsStatusAvailable => "Available",
        ModelsStatusMissing => "Missing",
        ModelsKeywordOnlyHint => {
            "Keyword search still works. Add a model to also \
             search by meaning."
        }
        ModelsVerification => "Verification",
        SettingsTitle => "Settings",
        SettingsLanguageHeading => "Language",
        SettingsPrivacyHeading => "Privacy",
        SettingsAdvancedHeading => "Advanced view",
        SettingsAdvancedHint => {
            "Show technical detail in search results, preparation, and storage."
        }
        SettingsPrivacyLocalOnly => "Documents are processed on this computer only.",
        SearchModeLabel => "Mode",
        SearchModeAuto => "Auto",
        SearchModeExact => "Keyword",
        SearchModeConceptual => "By meaning",
        BadgeKeyword => "Keyword",
        BadgeSemantic => "By meaning",
        BadgeReranked => "Reranked",
        BadgeSourceStale => "Needs update",
        DialogAddSourceTitle => "Select a folder to add",
        DialogChooseSearchFolderTitle => "Choose folder to search",
        WizardTitleNotConfigured => "Set up search by meaning",
        WizardTitleFileMissing => "Model not found",
        WizardTitleValidating => "Checking model folder",
        WizardTitleReady => "The model is ready to use",
        WizardBodyNotConfigured => {
            "Keyword search is ready. To also search by meaning, orbok needs \
             a local AI model on this computer. No files are uploaded; the \
             model runs on this computer."
        }
        WizardBodyFileMissing => {
            "The model folder is no longer at its expected location. This can \
             happen when a drive is disconnected or files are moved."
        }
        WizardBodyLocateExisting => {
            "Already have the model files? Point orbok at the folder that \
             contains them."
        }
        WizardBodyFilesIncomplete => "That folder is missing some of the required files.",
        WizardActionUseModel => "Use this model",
        WizardPathPlaceholder => "Folder path…",
        WizardDownloadAction => "Download from HuggingFace",
        WizardDownloadProgress => "Downloading model…",
        WizardActionSkip => "Skip — use keyword search only",
        WizardActionCancelDownload => "Cancel download",
        WizardCancellingDownload => "Cancelling…",
        WizardOr => "or",
        WizardMissingMarker => "missing",
        WizardBack => "Back",
        ModelConsentTitle => "Review model download",
        ModelConsentBody => {
            "orbok will contact the provider and save this model locally only after you continue."
        }
        ModelConsentPrivacy => {
            "Your documents, searches, folder paths, and this save location are not sent to the model provider."
        }
        ModelConsentProvider => "Provider",
        ModelConsentSource => "Source",
        ModelConsentRevision => "Version",
        ModelConsentExactSize => "Exact download size",
        ModelConsentLicense => "License",
        ModelConsentLocation => "Save location",
        ModelConsentVerification => "Verification",
        ModelTrustAppWillVerify => "orbok will verify the download before use",
        ModelTrustAppVerified => "App verified",
        ModelTrustUserSupplied => {
            "You provided this model. orbok cannot confirm where it came from."
        }
        ModelConsentConfirm => "Agree and download",
        ModelConsentCancel => "Back",
        ModelArtifactTokenizer => "Vocabulary",
        ModelArtifactOnnx => "Search model",
        ModelDeliveryStoreUnavailable => "The model store is busy or unavailable. Try again.",
        ModelDeliveryConnection => {
            "The download could not connect. Check your connection and try again."
        }
        ModelDeliveryVerification => "The downloaded files could not be verified. Try again.",
        ModelDeliveryLocalStorage => {
            "The model could not be saved safely. Check local storage and try again."
        }
        ModelDeliveryInternalState => "orbok could not continue the model setup safely. Try again.",
        ModelDeliveryCancelled => "The download was cancelled.",
        ModelPersistenceSaving => "Saving this model choice…",
        ModelPersistenceFailed => "The model is ready, but this choice could not be saved.",
        ModelPersistenceRetry => "Try saving again",
        ModelLoadFailed => "The model was saved, but it could not be loaded.",
        ModelLoadRetry => "Try again",
        ModelLoadFailedTitle => "Model could not be loaded",
        NoticeFolderFailTitle => "Folder was not added",
        NoticeFolderFailBody => {
            "We could not add that folder. Please choose another folder \
             or check that you can open it."
        }
        NoticeSearchFailTitle => "Search did not finish",
        NoticeSearchFailBody => "Something went wrong while searching. Please try again.",
        NoticeFolderAddedTitle => "Folder added",
        NoticeFolderAddedBody => "orbok is preparing your search now.",
        NoticeFolderAlreadyAddedTitle => "Folder already added",
        NoticeFolderAlreadyAddedBody => "This folder is already in your list.",
        NoticeFolderAlreadyIncludedTitle => "Folder already included",
        NoticeFoldersCombinedTitle => "Folders combined",
        NoticeSettingsFileUnreadableTitle => "Settings file could not be read",
        NoticeSettingsFileUnreadableBody => {
            "Your settings file is damaged, so orbok started with default settings. The damaged file was kept as settings.json.unreadable."
        }
        NoticeSearchReadyTitle => "Search is ready",
        NoticeSearchReadyBody => "Your files are ready to search.",
        NoticePreviewsClearedTitle => "Temporary previews cleared",
        NoticeSearchCacheClearedTitle => "Old search results cleared",
        NoticeExtractedTextClearedTitle => "Extracted text cleared",
        NoticeReplacedDataRemovedTitle => "Old data from updated files removed",
        NoticeCleanupBody => "Your files are never changed or deleted.",
        NoticeActionTryAgain => "Try again",
        NoticeActionChooseFolder => "Choose another folder",
        NoticeActionGoToFolders => "Go to Folders",
        NoticeActionShowInFolder => "Show in folder",
        NoticeFileNotFoundTitle => "This file could not be found",
        NoticeFileNotFoundBody => {
            "It may have been moved, renamed or deleted, or its drive may be disconnected."
        }
        NoticeFileNotOpenedTitle => "This file could not be opened",
        NoticeFileNotOpenedBody => "No app on this computer opened it.",
        NoticeFileNotAllowedBody => "orbok isn't allowed to open it.",
        NoticeFileCheckFailedBody => {
            "orbok could not check this file just now. Try again in a moment."
        }
        NoticeSettingSaveFailTitle => "Setting not saved",
        NoticeSettingSaveFailBody => {
            "Your change didn't save. It may not be there next time you open orbok."
        }
        NoticeResetFailTitle => "Reset didn't finish",
        NoticeResetFailBody => {
            "Some app data may not have been cleared. Try again, or check available storage space."
        }
        NoticeSourceRemoveFailTitle => "Folder not removed",
        NoticeSourceRemoveFailBody => "This folder is still registered. Try removing it again.",
        NoticeRecentSearchesNotClearedTitle => "Recent searches not cleared",
        NoticeRecentSearchesNotClearedBody => "The list could not be cleared. Try again.",
        NoticeRecentSearchNotRemovedTitle => "Recent search not removed",
        NoticeRecentSearchNotRemovedBody => "It is still in the list. Try again.",
        NoticeCleanupDidNotFinishTitle => "Cleanup didn't finish",
        NoticeCleanupDidNotFinishBody => {
            "Some of it may not have been removed. Your files are never changed or deleted. \
             Try again."
        }
        NoticeFolderNotCheckedTitle => "Folder not checked",
        NoticeFolderNotCheckedBody => "orbok could not check this folder for changes. Try again.",
        NoticeStorageUnavailableTitle => "Local storage unavailable",
        NoticeStorageUnavailableBody => {
            "orbok could not reach its local files just now. Check storage space and permissions, then try again."
        }
        NoticePreparationCouldNotStartTitle => "Search preparation isn't running",
        NoticePreparationCouldNotStartBody => {
            "orbok couldn't start getting your files ready to search. Restart orbok, or check that its local files are reachable."
        }
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
        AddSensitiveTitle => "Add a folder that may contain private files?",
        AddSensitiveConfirm => "Add anyway",
        NoticeDismiss => "Dismiss",
        Cancel => "Cancel",
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
        SearchFilesStillPreparing => "Some files are still being prepared.",
        SearchResultsWillImprove => "Results will improve as preparation finishes.",
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
        SourceActionRemoveFromOrbok => "Remove from orbok",
        SourceRemoveConfirmBody => {
            "Your files are never changed or deleted. orbok removes what it prepared to \
             search this folder, and prepares it again if you add it back."
        }
        SourceRemoveConfirm => "Remove",
        SourceFolderNotFoundDetail => {
            "This can happen if a drive is disconnected or the folder was moved."
        }
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
        ModelDownloadFailed => "Download did not finish",
        ModelDownloadRetry => "Try again",
        ModelDownloadingWhatNeeded => "Downloading what is needed...",
        // RFC-039: Privacy modes
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
        SearchLocationClear => "Clear this folder",
        SearchScopeOnly => "This folder only",
        SearchScopeSubfolders => "This folder and subfolders",
        SearchRecentFoldersLabel => "Recent folders",
        // RFC-042: search history
        RecentSearchesLabel => "Recent searches",
        SearchAgainButton => "Search again",
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
        NoRecentSearches => "No recent searches yet.",
        DroppedFilterNotice => "One narrowing choice was no longer available and was removed.",
        DiagnosticsAppVersion => "App version",
        DiagnosticsPlatformSummary => "Platform summary",
        DiagnosticsFolderStatusCounts => "Folder status counts",
        DiagnosticsSearchPreparationStatus => "Search preparation status",
        DiagnosticsModelReadiness => "Model readiness",
        DiagnosticsRedactedLogs => "Redacted logs",
        DiagnosticsDocuments => "Documents",
        DiagnosticsSearchWords => "Search words",
        DiagnosticsRawFolderPaths => "Raw folder paths",
        DiagnosticsIncludedHeading => "Included:",
        DiagnosticsNotIncludedHeading => "Not included:",
        DiagnosticsFolderNamesOptedIn => "Folder names (opted in)",
        StartupFailedTitle => "orbok could not start",
        StartupFailedNewerDataBody => {
            "This data was created by a newer version of orbok. Update orbok, then start it again."
        }
        StartupFailedOtherBody => "orbok could not open its data. Try starting it again.",
        StartupFailedClose => "Close",
    }
}
