//! UI translation. English strings are the keys, so any string without a translation simply
//! stays in English instead of showing an empty label. The active language is thread-local and
//! set once per frame from the applied config.
//!
//! This is deliberately a plain match instead of a runtime dictionary: it compiles to a jump
//! table, needs no allocation for hits, and makes missing translations obvious in review.

use std::cell::Cell;
use std::time::{Duration, SystemTime};

use rewind_core::i18n::Lang;

thread_local! {
    static LANG: Cell<Lang> = const { Cell::new(Lang::English) };
}

pub fn set_lang(l: Lang) {
    LANG.with(|c| c.set(l));
}

pub fn lang() -> Lang {
    LANG.with(|c| c.get())
}

/// Translate an English UI string. Unknown strings pass through unchanged.
pub fn t(s: &str) -> &str {
    if lang() == Lang::English {
        return s;
    }
    match s {
        // ---- navigation and pages
        "Instant replay" => "Anlık tekrar",
        "Replay" => "Tekrar",
        "Clips" => "Klipler",
        "Settings" => "Ayarlar",
        "Add-ons" => "Eklentiler",
        "About" => "Hakkında",

        // ---- replay page
        "Always recording in the background. One key keeps the moment you just had." => {
            "Arka planda her zaman kayıtta. Tek tuş, az önce yaşadığın anı saklar."
        }
        "Instant Replay" => "Anlık Tekrar",
        "Replay on" => "Tekrar açık",
        "Replay off" => "Tekrar kapalı",
        "RECORDING" => "KAYITTA",
        "PAUSED" => "DURAKLATILDI",
        "Not recording" => "Kayıt yok",
        "not recording" => "kayıt yok",
        "Off" => "Kapalı",
        "Game audio + mic" => "Oyun sesi + mikrofon",
        "Game audio" => "Oyun sesi",
        "Mic only" => "Yalnızca mikrofon",
        "No audio" => "Ses yok",
        "Recording on the CPU" => "CPU üzerinde kayıt",
        "That is normal on laptops without a hardware video encoder, and the clips still look good. If recording feels heavy, the Low-end preset keeps it light." => {
            "Donanım video kodlayıcısı olmayan dizüstü bilgisayarlarda bu normaldir ve klipler yine de iyi görünür. Kayıt ağır geliyorsa Düşük seviye ön ayarı onu hafif tutar."
        }
        "Apply Low-end preset" => "Düşük seviye ön ayarını uygula",
        "Turn on" => "Aç",
        "Turn off" => "Kapat",
        "Instant Replay is off" => "Anlık Tekrar kapalı",
        "Saving…" => "Kaydediliyor…",
        "Play last clip" => "Son klibi oynat",
        "MEMORY IN USE" => "KULLANILAN BELLEK",
        "CLIP SIZE" => "KLİP BOYUTU",
        "THIS APP" => "BU UYGULAMA",
        "while the buffer is full" => "tampon doluyken",
        "for {secs} seconds" => "{secs} saniye için",
        "Save last {secs} s" => "Son {secs} sn'yi kaydet",
        "Keeping the last {secs} seconds" => "Son {secs} saniye tutuluyor",
        "Last {secs} s" => "Son {secs} sn",
        "Press {key} while Instant Replay is on to save your first one." => {
            "İlk kaydını yapmak için Anlık Tekrar açıkken {key} tuşuna bas."
        },
        "of {time}" => "{time} /",
        "Recent clips" => "Son klipler",
        "View all" => "Tümünü gör",
        "No clips yet" => "Henüz klip yok",
        "Open clips folder" => "Klipler klasörünü aç",
        "Search clips" => "Kliplerde ara",
        "Nothing matches that search" => "Bu aramayla eşleşen yok",
        "Move to Recycle Bin" => "Geri Dönüşüm Kutusuna taşı",
        "Show in folder" => "Klasörde göster",
        "Refresh" => "Yenile",
        "Play" => "Oynat",
        "Needs ffmpeg next to Rewind.exe or on PATH" => "Rewind.exe yanında veya PATH'te ffmpeg gerekir",
        "{n} clips" => "{n} klip",

        // ---- settings: performance
        "Changes apply when you press Apply. Video changes restart the buffer for a moment." => {
            "Değişiklikler Uygula'ya basınca geçerli olur. Video değişiklikleri tamponu bir an yeniden başlatır."
        }
        "PERFORMANCE" => "PERFORMANS",
        "Smooth animations" => "Yumuşak animasyonlar",
        "Fades and slides in the interface. Turn off on slow PCs." => {
            "Arayüzde yumuşak geçişler. Yavaş bilgisayarlarda kapat."
        }
        "Smooth scrolling" => "Yumuşak kaydırma",
        "Glides the page between wheel notches instead of jumping." => {
            "Tekerlek adımları arasında sayfayı atlamak yerine kaydırır."
        }
        "Low-power interface" => "Düşük güç arayüzü",
        "No pulsing, slower refresh, cheaper video scaling." => {
            "Nabız efekti yok, daha yavaş yenileme, daha ucuz video ölçekleme."
        }
        "Toggle effects" => "Açma animasyonları",
        "Try it" => "Dene",
        "Shine" => "Parlama",
        "Sparkle" => "Işıltı",
        "Creeper" => "Creeper",
        "Ripple" => "Dalga",
        "Confetti" => "Konfeti",
        "Comet" => "Kuyruklu yıldız",
        "Bloom" => "Çiçek açımı",
        "Bolt" => "Şimşek",
        "No celebration." => "Kutlama yok.",
        "A soft light sweep across the switch." => "Düğmenin üzerinden yumuşak bir ışık geçişi.",
        "A burst of tiny stars." => "Küçük yıldızlardan bir patlama.",
        "A pixel creeper goes boom." => "Piksel bir creeper patlar.",
        "Calm expanding rings." => "Sakin genişleyen halkalar.",
        "Colourful paper thrown in the air." => "Havaya atılan renkli kâğıtlar.",
        "A bright streak with a glowing tail." => "Parlayan kuyruklu parlak bir çizgi.",
        "Petals unfold from a bright centre." => "Parlak bir merkezden açılan yapraklar.",
        "A quick spark of lightning." => "Hızlı bir şimşek kıvılcımı.",
        "Clip thumbnails" => "Klip küçük resimleri",
        "Uses Windows' own thumbnail cache; MP4 works best." => {
            "Windows'un kendi küçük resim önbelleğini kullanır; MP4 en iyi çalışır."
        }
        "Low-end PC" => "Düşük seviye PC",
        "Balanced" => "Dengeli",
        "High quality" => "Yüksek kalite",
        "720p30, light on CPU and GPU, no UI animations" => "720p30, CPU ve GPU'ya hafif, arayüz animasyonu yok",
        "1080p60, a good default for most PCs" => "1080p60, çoğu PC için iyi bir varsayılan",
        "Native resolution, high bitrate, for strong GPUs" => "Doğal çözünürlük, yüksek bit hızı, güçlü GPU'lar için",

        // ---- settings: video
        "VIDEO" => "VİDEO",
        "Monitor" => "Monitör",
        "Primary" => "Birincil",
        "Screen capture method" => "Ekran yakalama yöntemi",
        "Whole screen" => "Tüm ekran",
        "Games only" => "Yalnızca oyunlar",
        "Whole screen follows you into fullscreen games." => "Tüm ekran, tam ekran oyunlara seninle birlikte girer.",
        "DXGI (no border)" => "DXGI (kenarlıksız)",
        "Windows Graphics Capture" => "Windows Graphics Capture",
        "Best on Windows 10: fast and no yellow recording border." => {
            "Windows 10'da en iyisi: hızlı ve sarı kayıt çerçevesi yok."
        }
        "Needed on some Windows 11 setups, but Windows 10 draws a yellow border." => {
            "Bazı Windows 11 kurulumlarında gerekir, ancak Windows 10 sarı bir çerçeve çizer."
        }
        "Native" => "Doğal",
        "Frame rate" => "Kare hızı",
        "Resolution" => "Çözünürlük",
        "Same as monitor" => "Monitörle aynı",
        "Never upscales." => "Asla büyütmez.",
        "Codec" => "Kodek",
        "HEVC and AV1 give smaller files if your GPU supports them." => {
            "GPU'n destekliyorsa HEVC ve AV1 daha küçük dosyalar üretir."
        }
        "Encoder quality" => "Kodlayıcı kalitesi",
        "Fastest" => "En hızlı",
        "Best quality" => "En iyi kalite",
        "Bitrate" => "Bit hızı",
        "Higher looks better and makes bigger files." => "Yüksek değer daha iyi görünür ve daha büyük dosya üretir.",
        "Suggest" => "Öner",
        "Show mouse cursor" => "Fare imlecini göster",

        // ---- settings: audio
        "AUDIO" => "SES",
        "Game & desktop sound" => "Oyun ve masaüstü sesi",
        "Everything you hear from your speakers." => "Hoparlörlerinden duyduğun her şey.",
        "Desktop audio device" => "Masaüstü ses aygıtı",
        "The speakers or headphones to record. 'System default' follows Windows." => {
            "Kaydedilecek hoparlör veya kulaklık. 'Sistem varsayılanı' Windows'u takip eder."
        }
        "Microphone" => "Mikrofon",
        "Mixed into the same track as the game audio." => "Oyun sesiyle aynı parçaya karıştırılır.",
        "Microphone device" => "Mikrofon aygıtı",
        "Pick the exact mic you speak into. A missing device is the usual reason your voice is silent." => {
            "Konuştuğun mikrofonu tam olarak seç. Sesinin duyulmamasının olağan nedeni eksik aygıttır."
        }
        "Audio quality" => "Ses kalitesi",
        "Refresh devices" => "Aygıtları yenile",
        "System default" => "Sistem varsayılanı",
        "Missing device" => "Aygıt bulunamadı",
        "Audio device" => "Ses aygıtı",

        // ---- settings: buffer
        "REPLAY BUFFER" => "TEKRAR TAMPONU",
        "Replay length" => "Tekrar süresi",
        "Memory limit" => "Bellek sınırı",
        "File format" => "Dosya biçimi",
        "MP4 (most compatible)" => "MP4 (en uyumlu)",
        "MKV (safest if the PC crashes)" => "MKV (PC çökerse en güvenli)",
        "Ask before deleting a clip" => "Klip silmeden önce sor",
        "Turn off to send clips straight to the Recycle Bin with one click." => {
            "Tek tıkla doğrudan Geri Dönüşüm Kutusuna göndermek için kapat."
        }
        "Turn off to delete clips with one click." => "Klipleri tek tıkla silmek için kapat.",
        "Delete permanently" => "Kalıcı olarak sil",
        "Skip the Recycle Bin. Deleted clips can't be recovered." => {
            "Geri Dönüşüm Kutusunu atla. Silinen klipler geri getirilemez."
        }
        "Save folder" => "Kayıt klasörü",
        "Open folder" => "Klasörü aç",
        "Uses about {n} MB of RAM." => "Yaklaşık {n} MB RAM kullanır.",
        "Needs ~{n} MB, over your limit, so it will keep less than this." => {
            "~{n} MB gerekiyor, sınırını aşıyor, bu yüzden bundan daha azını tutacak."
        }

        // ---- settings: notifications
        "NOTIFICATIONS" => "BİLDİRİMLER",
        "Show notifications over games" => "Oyunların üzerinde bildirim göster",
        "Hidden from your recordings. Needs borderless or windowed games." => {
            "Kayıtlarında görünmez. Kenarlıksız veya pencereli oyunlar gerekir."
        }
        "Position" => "Konum",
        "Top left" => "Sol üst",
        "Top right" => "Sağ üst",
        "Bottom left" => "Sol alt",
        "Bottom right" => "Sağ alt",
        "Duration" => "Süre",
        "Play a sound when a clip is saved" => "Klip kaydedildiğinde ses çal",
        "Also works in exclusive-fullscreen games, where the overlay can't draw." => {
            "Kaplamanın çizemediği özel tam ekran oyunlarda da çalışır."
        }
        "Preview" => "Önizle",

        // ---- settings: hotkeys
        "HOTKEYS" => "KISAYOLLAR",
        "Click, then press the keys you want." => "Tıkla, sonra istediğin tuşlara bas.",
        "Save replay" => "Tekrarı kaydet",
        "Press the new keys…" => "Yeni tuşlara bas…",
        "Up" => "Yukarı",
        "Down" => "Aşağı",
        "Left" => "Sol",
        "Right" => "Sağ",

        // ---- settings: general
        "GENERAL" => "GENEL",
        "Language" => "Dil",
        "Interface language. English is the default." => "Arayüz dili. Varsayılan İngilizce'dir.",
        "Turn replay on/off" => "Tekrarı aç/kapat",
        "Start Instant Replay when Rewind opens" => "Rewind açıldığında Anlık Tekrar'ı başlat",
        "Recording and hotkeys keep working." => "Kayıt ve kısayollar çalışmaya devam eder.",
        "Closing the window keeps Rewind in the tray" => "Pencereyi kapatmak Rewind'ı tepside tutar",
        "Start hidden in the tray" => "Tepside gizli başlat",
        "Start with Windows" => "Windows ile başlat",
        "Apply" => "Uygula",
        "Revert" => "Geri al",
        "You have unsaved changes" => "Kaydedilmemiş değişiklikler var",
        "Settings applied" => "Ayarlar uygulandı",
        "Couldn't apply settings: {e}" => "Ayarlar uygulanamadı: {e}",
        "Settings applied but couldn't be saved: {e}" => "Ayarlar uygulandı ama kaydedilemedi: {e}",
        "Start with Windows: {e}" => "Windows ile başlatma: {e}",
        "Keeping the last {secs} s" => "Son {secs} sn tutuluyor",
        "Trimmed copy saved: {name}" => "Kırpılmış kopya kaydedildi: {name}",
        "Trim failed: {e}" => "Kırpma başarısız oldu: {e}",
        "English" => "English",
        "Türkçe" => "Türkçe",

        // ---- voice
        "VOICE CLIPPING" => "SESLE KLİP",
        "Speech engine" => "Konuşma motoru",
        "About 15 MB. Recognition runs on this PC; no audio is sent anywhere." => {
            "Yaklaşık 15 MB. Tanıma bu bilgisayarda çalışır; ses hiçbir yere gönderilmez."
        }
        "A small language model, about 40 MB each." => "Küçük bir dil modeli, her biri yaklaşık 40 MB.",
        "Save a clip when I say the phrase" => "Cümleyi söylediğimde klip kaydet",
        "Download the engine and a language first." => "Önce motoru ve bir dil indir.",
        "Trigger phrase" => "Tetik cümlesi",
        "Use words your language model knows, like \"clip that\" or \"klip al\"." => {
            "Dil modelinin bildiği kelimeler kullan, örneğin \"klip al\"."
        },
        "Use default" => "Varsayılanı kullan",
        "Sensitivity" => "Hassasiyet",
        "Higher picks up quieter speech but wakes more often." => {
            "Yüksek değer daha sessiz konuşmayı yakalar ama daha sık uyanır."
        }
        "Status" => "Durum",
        "Listening" => "Dinliyor",
        "Starting…" => "Başlıyor…",
        "Not running" => "Çalışmıyor",
        "Retry" => "Yeniden dene",
        "Tip: say the phrase on its own, with a short pause before and after; it doesn't count inside a longer sentence. Press Apply after changing the phrase or language." => {
            "İpucu: cümleyi tek başına, önünde ve arkasında kısa bir duraklamayla söyle; uzun bir cümlenin içinde sayılmaz. Cümleyi veya dili değiştirdikten sonra Uygula'ya bas."
        }
        "Installed" => "Kurulu",
        "Download" => "İndir",
        "Remove" => "Kaldır",
        "Use" => "Kullan",
        "In use" => "Kullanımda",
        "Built in" => "Yerleşik",
        "Optional download" => "İsteğe bağlı indirme",
        "by {author}" => "{author} tarafından",
        "by {author} · optional download" => "{author} tarafından · isteğe bağlı indirme",
        "Couldn't load the add-on list: {e}" => "Eklenti listesi yüklenemedi: {e}",
        "Checking for add-ons…" => "Eklentiler denetleniyor…",

        // ---- add-ons / themes
        "Optional extras. Nothing here is installed by default, so Rewind stays small." => {
            "İsteğe bağlı ekstralar. Buradakiler varsayılan olarak kurulu değildir, böylece Rewind küçük kalır."
        }
        "THEMES" => "TEMALAR",
        "CELEBRATION EFFECTS" => "KUTLAMA EFEKTLERİ",
        "Download effects in Add-ons first." => "Önce Eklentiler'den efekt indir.",
        "Download a celebration pack from Add-ons to turn these on." => {
            "Bunları açmak için Eklentiler'den bir kutlama paketi indir."
        }
        "Open Add-ons" => "Eklentileri aç",
        "No effect packs are available right now." => "Şu anda kullanılabilir efekt paketi yok.",
        "Download to unlock these celebrations." => "Bu kutlamaların kilidini açmak için indir.",
        "Mint (default)" => "Mint (varsayılan)",
        "Make your own: drop a theme .toml file into the themes folder, or start from the example." => {
            "Kendin yap: tema klasörüne bir .toml dosyası bırak veya örnekten başla."
        }
        "Open themes folder" => "Tema klasörünü aç",
        "Create example theme" => "Örnek tema oluştur",
        "The example is a fully commented copy of the default theme. Rename it, change the colours and press Apply." => {
            "Örnek, varsayılan temanın tamamen açıklamalı bir kopyasıdır. Adını değiştir, renkleri değiştir ve Uygula'ya bas."
        }
        "Created {name}.toml in the themes folder" => "Tema klasöründe {name}.toml oluşturuldu",
        "Couldn't create the example theme: {e}" => "Örnek tema oluşturulamadı: {e}",

        // ---- about
        "About Rewind" => "Rewind Hakkında",
        "A small instant-replay recorder written in Rust." => "Rust ile yazılmış küçük bir anlık tekrar kaydedici.",
        "Version" => "Sürüm",
        "Version {v}" => "Sürüm {v}",
        "Version {v} is available." => "{v} sürümü mevcut.",
        "Capture, encoding and the replay buffer are provided by libobs from OBS Studio (GPL-3.0-or-later), driven through the libobs-rs bindings. Rewind is licensed GPL-3.0-or-later and isn't affiliated with NVIDIA or the OBS Project." => {
            "Yakalama, kodlama ve tekrar tamponu OBS Studio'nun libobs kitaplığı (GPL-3.0-or-later) tarafından sağlanır ve libobs-rs bağlantılarıyla yönetilir. Rewind, GPL-3.0-or-later ile lisanslanmıştır ve NVIDIA veya OBS Project ile bağlantılı değildir."
        }
        "Clips never leave your PC. Network access is limited to the one-time OBS runtime download, optional add-ons, and the update check." => {
            "Klipler bilgisayarından ayrılmaz. Ağ erişimi tek seferlik OBS çalışma zamanı indirmesi, isteğe bağlı eklentiler ve güncelleme denetimiyle sınırlıdır."
        }
        "Typeface: Inter, SIL Open Font License 1.1." => "Yazı tipi: Inter, SIL Open Font License 1.1.",
        "UPDATES" => "GÜNCELLEMELER",
        "Check for updates" => "Güncellemeleri denetle",
        "Checking…" => "Denetleniyor…",
        "Not checked yet." => "Henüz denetlenmedi.",
        "Asking GitHub…" => "GitHub'a soruluyor…",
        "You're on the latest version." => "En son sürümü kullanıyorsun.",
        "Open release page" => "Sürüm sayfasını aç",
        "Couldn't check: {e}" => "Denetlenemedi: {e}",
        "What's new:" => "Yenilikler:",
        "Check automatically at launch" => "Açılışta otomatik denetle",
        "One small request to GitHub. No account or personal data is sent." => {
            "GitHub'a küçük bir istek. Hesap veya kişisel veri gönderilmez."
        },

        // ---- dialogs
        "Trim clip" => "Klibi kırp",
        "Lossless cut with no re-encoding, so the start snaps to the nearest keyframe." => {
            "Yeniden kodlama olmadan kayıpsız kesim, bu yüzden başlangıç en yakın ana kareye oturur."
        }
        "Keep only the last…" => "Yalnızca son…",
        "Keep from … to …" => "… ile … arasını tut",
        "A new file is saved next to the original." => "Orijinalin yanına yeni bir dosya kaydedilir.",
        "Trimming…" => "Kırpılıyor…",
        "to" => "ile",
        "Trim" => "Kırp",
        "Cancel" => "İptal",
        "Delete clip?" => "Klip silinsin mi?",
        "It will be moved to the Recycle Bin." => "Geri Dönüşüm Kutusuna taşınacak.",
        "Keep it" => "Vazgeç",
        "Delete" => "Sil",
        "Moved to Recycle Bin" => "Geri Dönüşüm Kutusuna taşındı",
        "It will be deleted permanently. This can't be undone." => {
            "Kalıcı olarak silinecek. Bu geri alınamaz."
        }
        "Clip deleted" => "Klip silindi",
        "Update now" => "Şimdi güncelle",
        "Size" => "Boyut",
        "Thumbnail size (Ctrl + mouse wheel)" => "Küçük resim boyutu (Ctrl + fare tekerleği)",
        "HOVER PREVIEWS" => "ÜZERİNE GELİNCE ÖNİZLEME",
        "Clip previews" => "Klip önizlemeleri",
        "Hover a clip's thumbnail to play its last 20 seconds. Also enables trimming." => {
            "Son 20 saniyesini oynatmak için klibin küçük resminin üzerine gel. Kırpmayı da açar."
        }
        "Plays the last 20 seconds of a clip when you hover its thumbnail, and enables trimming. Uses ffmpeg ({size})." => {
            "Küçük resmin üzerine gelince klibin son 20 saniyesini oynatır ve kırpmayı açar. ffmpeg kullanır ({size})."
        }
        "Not available from this release." => "Bu sürümde yok.",
        "Play previews on hover" => "Üzerine gelince önizleme oynat",
        "Frames are made once per clip, at low priority, and cached." => {
            "Kareler her klip için bir kez, düşük öncelikle hazırlanır ve saklanır."
        }
        "No preview" => "Önizleme yok",
        "Copy" => "Kopyala",
        "Copy error" => "Hatayı kopyala",
        "Copied to clipboard" => "Panoya kopyalandı",
        "Copy diagnostics" => "Tanılama bilgisini kopyala",
        "Something not working?" => "Bir şey çalışmıyor mu?",
        "Copies version, settings, encoder, voice status and recent errors (no clips, no personal files) so you can paste them into a bug report." => {
            "Sürüm, ayarlar, kodlayıcı, ses durumu ve son hataları kopyalar (klip ya da kişisel dosya yok), hata bildirimine yapıştırabilirsin."
        }
        "Last heard:" => "Son duyulan:",
        "Saved a clip" => "Klip kaydedildi",
        "Ignored: say it on its own" => "Sayılmadı: tek başına söyle",
        "Ignored: not sure enough, say it clearer" => "Sayılmadı: emin olamadı, daha net söyle",
        "Ignored: a clip was just saved" => "Sayılmadı: az önce klip kaydedildi",
        "Not the phrase" => "Tetikleyici cümle değil",
        "Try again" => "Tekrar dene",
        "Updating Rewind" => "Rewind güncelleniyor",
        "Downloading version {v}…" => "{v} sürümü indiriliyor…",
        "Downloading version {v}… {p}%" => "{v} sürümü indiriliyor… %{p}",
        "Restarting into version {v}…" => "{v} sürümüyle yeniden başlatılıyor…",
        "Couldn't update: {e}" => "Güncellenemedi: {e}",
        "Rewind updated to version {v}" => "Rewind {v} sürümüne güncellendi",
        "Install updates automatically" => "Güncellemeleri otomatik yükle",
        "Downloads the new version, checks it and restarts Rewind. Off: just tell me and link to the release page." => {
            "Yeni sürümü indirir, doğrular ve Rewind'ı yeniden başlatır. Kapalı: sadece haber ver ve sürüm sayfasına yönlendir."
        }
        "Couldn't delete: {e}" => "Silinemedi: {e}",
        "Rename" => "Yeniden adlandır",
        "Rename clip" => "Klibi yeniden adlandır",
        "New name" => "Yeni ad",
        "Clip renamed" => "Klip yeniden adlandırıldı",
        "The file keeps its folder and format." => "Dosya klasörünü ve biçimini korur.",
        "Give the clip a name first." => "Önce klibe bir ad ver.",
        "That name can't be used. Try letters and numbers." => {
            "Bu ad kullanılamaz. Harf ve rakam dene."
        }
        "The clip's folder is missing." => "Klibin klasörü bulunamadı.",

        // ---- startup / failure
        "Setting things up" => "Hazırlanıyor",
        "Downloading the OBS capture runtime. This only happens once (about 100 MB)." => {
            "OBS yakalama çalışma zamanı indiriliyor. Bu yalnızca bir kez olur (yaklaşık 100 MB)."
        }
        "Starting" => "Başlatılıyor",
        "Spinning up the capture engine…" => "Yakalama motoru başlatılıyor…",
        "Rewind couldn't start" => "Rewind başlatılamadı",
        "Check your internet connection (needed once to fetch OBS) and restart the app." => {
            "İnternet bağlantını denetle (OBS'yi bir kez indirmek için gerekir) ve uygulamayı yeniden başlat."
        }

        // ---- notifications
        "Replay saved" => "Tekrar kaydedildi",
        "This is how notifications look" => "Bildirimler böyle görünür",
        "Couldn't save replay" => "Tekrar kaydedilemedi",
        "Instant Replay on" => "Anlık Tekrar açık",
        "Instant Replay off" => "Anlık Tekrar kapalı",
        "Couldn't change Instant Replay" => "Anlık Tekrar değiştirilemedi",
        "Global hotkeys are unavailable on this system." => "Bu sistemde genel kısayollar kullanılamıyor.",
        "Rewind is still running" => "Rewind hâlâ çalışıyor",
        "Find it in the system tray" => "Sistem tepsisinde bulabilirsin",
        "ffmpeg failed" => "ffmpeg başarısız oldu",
        "Installed {key}" => "{key} kuruldu",
        "Couldn't install {key}: {e}" => "{key} kurulamadı: {e}",

        // ---- misc
        "and" => "ve",
        _ => s,
    }
}

/// Translate a template and fill `{name}` placeholders. Use for strings that change per
/// second or per file, e.g. `tf("Save last {secs} s", &[("secs", "30")])`.
pub fn tf(s: &str, vars: &[(&str, &str)]) -> String {
    let mut out = t(s).to_string();
    for (k, v) in vars {
        out = out.replace(&format!("{{{k}}}"), v);
    }
    out
}

/// Relative age of a clip, localised. Mirrors `rewind_core::library::human_age`.
pub fn age(now: SystemTime, then: SystemTime) -> String {
    let secs = now.duration_since(then).unwrap_or(Duration::ZERO).as_secs();
    match lang() {
        Lang::English => rewind_core::library::human_age(now, then),
        Lang::Turkish => match secs {
            0..=9 => "az önce".into(),
            10..=59 => format!("{secs} sn önce"),
            60..=3_599 => format!("{} dk önce", secs / 60),
            3_600..=86_399 => format!("{} sa önce", secs / 3_600),
            _ => format!("{} gün önce", secs / 86_400),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_identity_and_turkish_translates() {
        set_lang(Lang::English);
        assert_eq!(t("Settings"), "Settings");
        assert_eq!(
            t("a string with no translation"),
            "a string with no translation"
        );

        set_lang(Lang::Turkish);
        assert_eq!(t("Settings"), "Ayarlar");
        assert_eq!(t("Save last {secs} s"), "Son {secs} sn'yi kaydet");
        assert_eq!(
            t("a string with no translation"),
            "a string with no translation"
        );
        assert_eq!(
            tf("Save last {secs} s", &[("secs", "30")]),
            "Son 30 sn'yi kaydet"
        );
        assert_eq!(
            tf("Couldn't check: {e}", &[("e", "offline")]),
            "Denetlenemedi: offline"
        );

        // Restore so other tests (and the default) see English.
        set_lang(Lang::English);
    }
}
