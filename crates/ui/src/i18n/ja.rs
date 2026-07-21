//! Japanese catalog (RFC-031). Exhaustive over [`MessageKey`].

use super::MessageKey;

pub fn message(key: MessageKey) -> &'static str {
    use MessageKey::*;
    match key {
        AppTitle => "orbok",
        LocalOnlyBadge => "ローカル専用",
        NavSearch => "検索",
        NavSources => "フォルダー",
        NavIndexing => "準備",
        NavStorage => "ストレージ",
        NavModels => "モデル",
        NavAi => "AI",
        NavSettings => "設定",
        SearchPlaceholder => "ローカル文書を検索...",
        SearchButton => "検索",
        SearchNoSourcesTitle => "検索対象がありません",
        SearchNoSourcesBody => {
            "フォルダーまたはファイルを追加すると、orbok がローカル検索\
             インデックスを作成します。"
        }
        SearchAddSource => "ソースを追加",
        SearchNoResults => "結果が見つかりません",
        SearchKeywordOnlyNotice => {
            "セマンティック検索は利用できません。キーワード検索は使用できます。"
        }
        SourcesTitle => "フォルダー",
        SourcesEmptyTitle => "フォルダーが追加されていません",
        SourcesEmptyBody => {
            "orbok に検索を許可するフォルダーまたはファイルを追加してください。\
             orbok がコンピューター全体を自動的にスキャンすることはありません。"
        }
        SourcesAddFolder => "フォルダーを追加",
        SourcesStatusActive => "有効",
        SourcesStatusPaused => "一時停止",
        SourcesStatusMissing => "見つかりません",
        IndexingTitle => "インデックス",
        IndexingIdle => "検索の準備ができました",
        IndexingHealthIndexed => "済み",
        IndexingHealthStale => "要更新",
        IndexingHealthFailed => "失敗",
        IndexingHealthQueued => "待機中",
        StorageTitle => "ストレージ",
        StorageIntro => "orbok の保存内容を確認し、安全にクリーンアップできます。",
        StorageGroupSearchIndex => "検索データ",
        StorageGroupModels => "検索ヘルパー",
        StorageGroupCaches => "一時プレビュー",
        StorageSafeCleanupHeading => "安全なクリーンアップ",
        StorageClearSnippets => "一時スニペットを削除",
        StorageClearSearchCache => "期限切れの検索キャッシュを削除",
        StorageDangerHeading => "危険な操作",
        StorageResetCatalog => "カタログをリセット...",
        StorageResetWarning => {
            "登録済みソースとすべてのインデックスを削除します。\
             元のファイルが削除されることはありません。"
        }
        ModelsTitle => "モデル",
        ModelsEmbeddingRole => "埋め込み",
        ModelsRerankerRole => "リランカー",
        ModelsStatusAvailable => "利用可能",
        ModelsStatusMissing => "未導入",
        ModelsKeywordOnlyHint => {
            "キーワード検索は使用できます。概念的な検索を有効にするには、\
             埋め込みモデルを導入してください。"
        }
        ModelsVerification => "検証状態",
        SettingsTitle => "設定",
        SettingsLanguageHeading => "言語",
        SettingsPrivacyHeading => "プライバシー",
        SettingsAdvancedHeading => "詳細表示",
        SettingsAdvancedOn => "詳細表示: オン",
        SettingsAdvancedOff => "詳細表示: オフ",
        SettingsAdvancedHint => "検索結果・インデックス・ストレージに技術的な詳細を表示します。",
        SettingsPrivacyLocalOnly => "文書はこのコンピューター上でのみ処理されます。",
        SearchModeLabel => "モード",
        SearchModeAuto => "自動",
        SearchModeExact => "完全一致",
        SearchModeConceptual => "意味検索",
        SearchModeFast => "高速",
        BadgeKeyword => "キーワード",
        BadgeSemantic => "セマンティック",
        BadgeFused => "融合",
        WizardTitleNotConfigured => "セマンティック検索の設定",
        WizardTitleFileMissing => "埋め込みモデルが見つかりません",
        WizardTitleValidating => "モデルフォルダを確認中",
        WizardTitleReady => "埋め込みモデルの準備完了",
        WizardBodyNotConfigured => {
            "キーワード検索は利用可能です。意味による検索を使用するには、             このコンピュータにローカルAIモデルが必要です。             ファイルはアップロードされません。"
        }
        WizardBodyFileMissing => {
            "モデルフォルダが指定された場所にありません。             ドライブが切断されたか、ファイルが移動した可能性があります。"
        }
        WizardFilesNeededLabel => "フォルダ内の必要ファイル:",
        WizardDownloadHint => {
            "ダウンロード: huggingface-cli download intfloat/multilingual-e5-small"
        }
        WizardPathInputPlaceholder => "モデルフォルダのパス (例: ~/models/multilingual-e5-small)",
        WizardActionLocate => "モデルフォルダを選択",
        WizardActionValidate => "検証",
        WizardActionUseModel => "このモデルを使用",
        WizardActionContinue => "orbok を開始",
        WizardPathPlaceholder => "フォルダのパス…",
        WizardDownloadAction => "HuggingFaceからダウンロード",
        WizardDownloadProgress => "モデルをダウンロード中…",
        WizardActionSkip => "スキップ — キーワード検索のみ使用",
        WizardOr => "または",
        WizardMissingMarker => "不足",
        WizardBack => "戻る",
        WizardPreviousPathLabel => "最後の既知のパス",
        WizardValidationOk => "確認済み",
        WizardValidationFail => "見つかりません",
        WizardReadyBody => "セマンティック検索が利用可能になりました。",
        ModelConsentTitle => "モデルのダウンロードを確認",
        ModelConsentBody => {
            "続行すると、orbok は提供元に接続し、このモデルをローカルに保存します。"
        }
        ModelConsentPrivacy => {
            "文書、検索内容、検索元のパス、この保存場所はモデル提供元に送信されません。"
        }
        ModelConsentProvider => "提供元",
        ModelConsentSource => "ソース",
        ModelConsentRevision => "変更されないリビジョン",
        ModelConsentExactSize => "正確なダウンロードサイズ",
        ModelConsentLicense => "ライセンス",
        ModelConsentLocation => "保存場所",
        ModelConsentVerification => "検証状態",
        ModelTrustAppWillVerify => "使用前に orbok がダウンロードを検証",
        ModelTrustAppVerified => "アプリによる検証",
        ModelTrustUserSupplied => "ユーザー提供 / 出所未検証",
        ModelConsentConfirm => "同意してダウンロード",
        ModelConsentCancel => "戻る",
        ModelArtifactTokenizer => "トークナイザー",
        ModelArtifactOnnx => "検索モデル",
        ModelDeliveryStoreUnavailable => {
            "モデル保存領域を使用できないか、使用中です。もう一度お試しください。"
        }
        ModelDeliveryConnection => {
            "ダウンロードに接続できませんでした。接続を確認してもう一度お試しください。"
        }
        ModelDeliveryVerification => {
            "ダウンロードしたファイルを検証できませんでした。もう一度お試しください。"
        }
        ModelDeliveryLocalStorage => {
            "モデルを安全に保存できませんでした。ローカルストレージを確認してもう一度お試しください。"
        }
        ModelDeliveryInternalState => {
            "モデルの設定を安全に続行できませんでした。もう一度お試しください。"
        }
        ModelPersistenceSaving => "このモデル設定を保存中…",
        ModelPersistenceFailed => "モデルは準備できましたが、この設定を保存できませんでした。",
        ModelPersistenceRetry => "保存をもう一度試す",
        NoticeDownloadFailTitle => "ダウンロードが完了しませんでした",
        NoticeDownloadFailBody => {
            "ダウンロードを完了できませんでした。接続を確認して、もう一度お試しください。"
        }
        NoticeFolderFailTitle => "フォルダを追加できませんでした",
        NoticeFolderFailBody => {
            "そのフォルダを追加できませんでした。別のフォルダを選ぶか、開けるか確認してください。"
        }
        NoticeSearchFailTitle => "検索が完了しませんでした",
        NoticeSearchFailBody => "検索中に問題が発生しました。もう一度お試しください。",
        NoticeFilesMissingTitle => "ファイルが移動した可能性があります",
        NoticeFilesMissingBody => {
            "一部のファイルが見つかりません。ドライブが取り外されたか、ファイルが移動された可能性があります。"
        }
        NoticeFolderAddedTitle => "フォルダを追加しました",
        NoticeFolderAddedBody => "検索の準備をしています。",
        NoticeSearchReadyTitle => "検索の準備ができました",
        NoticeSearchReadyBody => "ファイルを検索できます。",
        NoticePreviewsClearedTitle => "一時プレビューを削除しました",
        NoticePreviewsClearedBody => "空き容量を増やしました。ファイルはそのままです。",
        NoticeActionTryAgain => "もう一度試す",
        NoticeActionChooseFolder => "別のフォルダを選ぶ",
        SettingsThemeHeading => "テーマ",
        ThemeSystem => "システムに合わせる",
        ThemeLight => "ライト",
        ThemeDark => "ダーク",
        ThemeHighContrastLight => "ハイコントラスト（ライト）",
        ThemeHighContrastDark => "ハイコントラスト（ダーク）",
        SettingsTextScaleHeading => "文字サイズ",
        TextScaleDefault => "標準",
        TextScaleLarge => "大",
        TextScaleLarger => "特大",
        SettingsReduceMotion => "モーションを減らす",
        SettingsReduceMotionHint => "アニメーションとトランジションを減らします。",
        SettingsCvdNote => {
            "ステータスカラーは常にラベルとアイコンとともに表示されるため、色覚に関わらず識別できます。"
        }
        NoticeSensitiveSourceTitle => "このフォルダには機密ファイルが含まれている可能性があります",
        NoticeSensitiveSourceBody => {
            "SSH鍵、ブラウザのプロフィール、またはその他の機密データが含まれている可能性があります。フォルダは追加されました。意図しない場合は削除してください。"
        }
        NoticeDismiss => "閉じる",
        Cancel => "キャンセル",
        Confirm => "確認",
        // RFC-041: Search, Narrow Results, Browse Around
        SearchNarrowResults => "結果を絞り込む",
        SearchNarrowedBy => "絞り込み条件",
        SearchMoreWays => "他の絞り込み方法",
        SearchClearFilters => "クリア",
        SearchNoResultsFiltered => "この条件では結果がありません",
        SearchNoResultsFilteredBody => "条件を一つ外してみてください。",
        SearchInThisFolder => "このフォルダ内を検索",
        SearchShowNearby => "近くのファイルを表示",
        SearchShowSimilar => "類似ファイルを表示",
        SearchResultsUpdating => "結果を更新中...",
        SearchPreparingFolder => "「{folder}」の検索準備中",
        SearchPartialReadiness => "{ready} 件のファイルが準備完了。今すぐ検索できます。",
        // RFC-041 filter labels
        FilterKind => "種類",
        FilterChanged => "変更日",
        FilterSearchIn => "検索対象",
        FilterReadyStatus => "準備状態",
        FilterKindPdfs => "PDF",
        FilterKindNotes => "メモ",
        FilterKindCode => "コード",
        FilterKindDocuments => "ドキュメント",
        FilterKindSpreadsheets => "スプレッドシート",
        FilterChangedToday => "今日",
        FilterChangedThisWeek => "今週",
        FilterChangedThisMonth => "今月",
        FilterChangedAnyTime => "すべての期間",
        FilterAllFolders => "すべてのフォルダ",
        // RFC-037: Source lifecycle
        SourceStateReady => "準備完了",
        SourceStatePreparing => "準備中",
        SourceStateNeedsUpdate => "更新が必要",
        SourceStatePaused => "一時停止中",
        SourceStateFolderNotFound => "フォルダが見つかりません",
        SourceStateCannotOpen => "開けません",
        SourceStateRemoved => "削除済み",
        SourceActionCheckAgain => "再確認",
        SourceActionPrepareAgain => "再準備",
        SourceActionChooseFolderAgain => "フォルダを選び直す",
        SourceActionRemoveFromOrbok => "orbokから削除",
        SourceFolderNotFoundDetail => {
            "ドライブが切断されたか、フォルダが移動された可能性があります。"
        }
        SourceFilesNotDeletedNotice => {
            "ファイルは削除されていません。orbokがこのフォルダを見つけられないだけです。"
        }
        SourceManyFilesChanged => "多くのファイルが変更されました。orbokが徐々に準備します。",
        SourcePausePreparation => "準備を一時停止",
        SourceResumePreparation => "準備を再開",
        // RFC-038: Result trust badges and recovery
        TrustNeedsUpdate => "更新が必要",
        TrustFileNotFound => "ファイルが見つかりません",
        TrustStillBeingPrepared => "準備中",
        TrustPartlyPrepared => "一部のみ準備済み",
        TrustCannotOpen => "開けません",
        TrustActionPrepareAgain => "再準備",
        TrustActionCheckFolder => "フォルダを確認",
        TrustActionRemoveFromResults => "結果から削除",
        TrustActionOpenAnyway => "そのまま開く",
        TrustActionShowInFolder => "フォルダで表示",
        TrustActionViewDetails => "詳細を表示",
        TrustFileChangedDetail => "このファイルは準備後に変更されました。",
        TrustFileNotFoundDetail => {
            "ファイルが見つかりません。移動、削除、またはドライブが切断された可能性があります。"
        }
        TrustPartlyPreparedDetail => "このファイルの一部のみが準備されました。",
        TrustScannedPdfDetail => {
            "このPDFには選択できないテキストの代わりに画像が含まれている可能性があります。"
        }
        TrustSomePagesDetail => "一部のページを準備できませんでした。",
        TrustSizeLimitDetail => "この大きなファイルの一部のみが準備されました。",
        TrustCannotOpenDetail => "orbokはこのファイルを開けません。",
        // RFC-043: Model download readiness
        ModelCheckingFiles => "検索ヘルパーを確認中...",
        ModelAlreadyReady => "より良い検索が使えます。",
        ModelNeedsDownload => {
            "検索ヘルパーファイルが必要です。orbokは不足しているものだけをダウンロードします。"
        }
        ModelDownloadingBetterSearch => "より良い検索をダウンロード中",
        ModelFilesStayLocal => "ファイルはこのコンピューターに保存されます。",
        ModelDownloadFailed => {
            "ダウンロードが完了しませんでした。接続を確認してもう一度お試しください。"
        }
        ModelDownloadRetry => "もう一度試す",
        ModelRepairingFiles => {
            "一部の検索ヘルパーファイルを修復する必要があります。orbokは必要なものだけをダウンロードします。"
        }
        ModelBasicSearchAvailable => "基本検索は使えます。意味による検索は後で追加できます。",
        ModelDownloadingWhatNeeded => "必要なものをダウンロード中...",
        // RFC-039: Privacy modes
        PrivacyTitle => "プライバシー",
        PrivacyLocalOnlyStatement => "ドキュメントはこのコンピューターでのみ処理されます。",
        PrivacyModeStandard => "標準",
        PrivacyModeStrict => "厳格",
        PrivacyModePortable => "ポータブル",
        PrivacyModeStrictDescription => "厳格プライバシーはorbokが記憶する内容を減らします。",
        PrivacyModePortableDescription => "orbokはアプリのコピーの隣にデータを保存します。",
        PrivacyRememberSearches => "最近の検索を記憶する",
        PrivacyRememberSearchesHint => "最近の検索はこのコンピューターにのみ保存されます。",
        PrivacySearchesDisabledStrict => "厳格プライバシーが有効な間は最近の検索は保存されません。",
        PrivacyTemporaryPreviews => "一時プレビュー",
        PrivacyTemporaryPreviewsHint => {
            "一時プレビューにより結果が速く開きます。いつでも削除できます。"
        }
        PrivacyClearPreviews => "一時プレビューを消去",
        PrivacyEnableStrictConfirm => "厳格プライバシーを有効にしますか？",
        PrivacyEnableStrictBody => {
            "orbokは最近の検索の保存を停止し、一時プレビューを減らします。保存済みのデータも消去できます。"
        }
        PrivacyTurnOn => "有効にする",
        PrivacyTurnOnAndClear => "有効にして消去",
        PrivacyFilesNotDeleted => "あなたのファイルは削除されません。",
        PrivacyModelDownloadNote => {
            "orbokは検索ヘルパーをダウンロードしますが、あなたのドキュメントはアップロードされません。"
        }
        // RFC-040: Diagnostics
        DiagnosticsTitle => "診断",
        DiagnosticsIntro => {
            "何か問題がある場合はサポートファイルを作成してください。デフォルトではドキュメントや検索ワードは含まれません。"
        }
        DiagnosticsCreateFile => "サポートファイルを作成",
        DiagnosticsPreviewTitle => "サポートファイルを作成",
        DiagnosticsIncludedLabel => "含まれる内容",
        DiagnosticsExcludedLabel => "含まれない内容",
        DiagnosticsOptInFolderNames => "フォルダ名を含める",
        DiagnosticsOptInFolderNamesHint => "使用しているフォルダが明らかになる可能性があります。",
        DiagnosticsOptInSearchWords => "最近の検索ワードを含める",
        DiagnosticsOptInSearchWordsHint => "何を検索していたかが明らかになる可能性があります。",
        DiagnosticsFileCreated => "サポートファイルが作成されました。",
        DiagnosticsShowFile => "ファイルを表示",
        DiagnosticsCreateFailed => {
            "サポートファイルを作成できませんでした。別の場所を選ぶか、もう一度お試しください。"
        }
        // RFC-045: search-in-folder flow
        SearchInLabel => "検索場所",
        SearchChooseFolder => "フォルダーを選択",
        SearchScopeOnly => "このフォルダーのみ",
        SearchScopeSubfolders => "このフォルダーとサブフォルダー",
        SearchRecentFoldersLabel => "最近のフォルダー",
        // RFC-042: search history
        RecentSearchesLabel => "最近の検索",
        SearchAgainButton => "もう一度検索",
        SearchingAgainStatus => "再検索中\u{2026}",
        OpenRecentSearches => "最近の検索",
        ClearRecentSearches => "最近の検索を消去",
        ClearRecentSearchesConfirmTitle => "最近の検索を消去しますか？",
        ClearRecentSearchesConfirmBody => {
            "orbokに表示されている検索履歴を削除します。\
             ファイルや検索データは削除されません。"
        }
        RecentSearchesClearedNotice => "最近の検索を消去しました。",
        RememberRecentSearches => "最近の検索を記憶する",
        RecentSearchesPrivacyNote => "最近の検索はこのコンピューター上にのみ保存されます。",
        RecentSearchesStrictPrivacyNote => {
            "厳格なプライバシーがオンの間、最近の検索は保存されません。"
        }
        NoRecentSearches => "最近の検索はまだありません。",
        DroppedFilterNotice => "利用できなくなった絞り込み条件が1つ削除されました。",
    }
}
