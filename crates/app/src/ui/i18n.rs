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
        "No effect." => "Efekt yok.",
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
        "Use words your language model knows, like \"clip that\" or \"klip al\". Separate several with commas: \"klip al, klibi al\"." => {
            "Dil modelinin bildiği kelimeler kullan, örneğin \"klip al\". Birden fazlasını virgülle ayır: \"klip al, klibi al\"."
        }
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
        "TOGGLE EFFECTS" => "AÇMA ANİMASYONLARI",
        "Extra animations" => "Ek animasyonlar",
        "Thumbnails zoom on hover, starring pops, a new clip glows for a moment. Needs smooth animations." => {
            "Küçük resimler üzerine gelince yakınlaşır, yıldızlama hareketlenir, yeni klip bir an parlar. Akıcı animasyonlar açık olmalı."
        }
        "Download effects in Add-ons first." => "Önce Eklentiler'den efekt indir.",
        "Download the toggle effects from Add-ons to turn these on." => {
            "Bunları açmak için Eklentiler'den açma animasyonlarını indir."
        }
        "Open Add-ons" => "Eklentileri aç",
        "No effect packs are available right now." => "Şu anda kullanılabilir efekt paketi yok.",
        "Download to unlock these toggle effects." => "Bu açma animasyonlarını açmak için indir.",
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
        "Couldn't save replay — {e}" => "Tekrar kaydedilemedi — {e}",
        "Capture" => "Yakalama",
        "DXGI screen capture wasn't available on this PC, so Rewind switched to Windows Graphics Capture. On Windows 10 that may draw a yellow border around the screen." => "Bu bilgisayarda DXGI ekran yakalama kullanılamadı, bu yüzden Rewind Windows Graphics Capture'a geçti. Windows 10'da bu, ekranın çevresinde sarı bir çerçeve gösterebilir.",
        "This PC has no hardware video encoder, so Rewind records H.264 on the CPU. That is expected on older laptops and still looks good. If it feels heavy, the Low-end preset keeps it light." => "Bu bilgisayarda donanımsal video kodlayıcı yok, bu yüzden Rewind H.264'ü işlemcide kaydediyor. Eski dizüstülerde bu normaldir ve görüntü yine iyidir. Ağır gelirse Düşük seviye PC ön ayarı yükü hafifletir.",
        "This PC has no hardware video encoder, so Rewind records H.264 on the CPU. That is expected on older laptops, and the Low-end preset is already keeping it light." => "Bu bilgisayarda donanımsal video kodlayıcı yok, bu yüzden Rewind H.264'ü işlemcide kaydediyor. Eski dizüstülerde bu normaldir ve Düşük seviye PC ön ayarı yükü zaten hafif tutuyor.",
        "Hardware encoding for this codec isn't available on this PC, so Rewind is recording H.264 on the CPU instead. The clips play everywhere and still look good." => "Bu kodek için donanımsal kodlama bu bilgisayarda yok, bu yüzden Rewind bunun yerine H.264'ü işlemcide kaydediyor. Klipler her yerde oynar ve yine iyi görünür.",
        "OBS didn't finish writing the clip. The replay buffer is still on; try again in a moment. If it keeps happening, check that the clip folder is writable." => "OBS klibi yazmayı bitiremedi. Tekrar tamponu hâlâ açık; birazdan tekrar dene. Sürekli oluyorsa klip klasörünün yazılabilir olduğunu kontrol et.",
        "replay buffer is off — turn it on first" => "tekrar tamponu kapalı — önce aç",
        "replay buffer not created" => "tekrar tamponu oluşturulmadı",
        "no monitors found" => "ekran bulunamadı",
        "couldn't start screen capture" => "ekran yakalama başlatılamadı",
        "no microphone found" => "mikrofon bulunamadı",
        "the voice add-on isn't installed" => "ses eklentisi kurulu değil",
        "couldn't load the language model (re-download it from Add-ons)" => "dil modeli yüklenemedi (Eklentiler'den yeniden indir)",
        "the trigger phrase is empty" => "tetikleyici cümle boş",
        "couldn't start the recogniser" => "tanıyıcı başlatılamadı",
        "no response from the server (are you online?)" => "sunucudan yanıt yok (internete bağlı mısın?)",
        "the connection dropped" => "bağlantı koptu",
        "checksum mismatch: the download is corrupted or has been tampered with" => "sağlama toplamı uyuşmuyor: indirilen dosya bozuk ya da değiştirilmiş",
        "missing checksum, refusing to download" => "sağlama toplamı yok, indirme reddedildi",
        "file is larger than allowed" => "dosya izin verilenden büyük",
        "this release has no checksum list" => "bu sürümde sağlama toplamı listesi yok",
        "this release doesn't have a file for this kind of install" => "bu sürümde bu kurulum türü için dosya yok",
        "the latest release has no version tag" => "son sürümün sürüm etiketi yok",
        "only .mp3 and .wav files can be used" => "sadece .mp3 ve .wav dosyaları kullanılabilir",
        "that file is over 10 MB; pick a short sound" => "dosya 10 MB'tan büyük; kısa bir ses seç",
        "you already have 5 sounds; remove one, or turn off the limit in Add-ons" => "zaten 5 sesin var; birini kaldır ya da Eklentiler'den sınırı kapat",
        "the clipboard is busy, try again" => "pano meşgul, tekrar dene",
        "couldn't put the file on the clipboard" => "dosya panoya konamadı",
        "the clip is too short" => "klip çok kısa",
        "interrupted" => "yarıda kesildi",
        "Open Rewind" => "Rewind'ı aç",
        "Save replay now" => "Tekrarı şimdi kaydet",
        "Quit Rewind" => "Rewind'dan çık",
        "Rewind — Instant Replay is on" => "Rewind — Anlık tekrar açık",
        "Rewind — Instant Replay is off" => "Rewind — Anlık tekrar kapalı",
        "{n} effects · Installed" => "{n} efekt · Kurulu",
        "{n} effects · by {author}" => "{n} efekt · {author} tarafından",
        "· made by pro82xw" => "· pro82xw tarafından yapıldı",
        "Whisper isn't answering" => "Whisper yanıt vermiyor",
        "the Whisper model is missing" => "Whisper modeli eksik",
        "Desktop audio unavailable: {e}" => "Masaüstü sesi kullanılamıyor: {e}",
        "Microphone unavailable: {e}" => "Mikrofon kullanılamıyor: {e}",
        "Game capture unavailable ({e}); recording the monitor only." => "Oyun yakalama kullanılamıyor ({e}); sadece ekran kaydediliyor.",
        "{h} couldn't be registered ({e}). Another app may be using it." => "{h} kaydedilemedi ({e}). Başka bir uygulama kullanıyor olabilir.",
        "Show only starred clips" => "Sadece yıldızlı klipleri göster",
        "No starred clips" => "Yıldızlı klip yok",
        "Star a clip to keep it here. Starred clips are never auto-deleted." => {
            "Burada tutmak için bir klibe yıldız ver. Yıldızlı klipler asla otomatik silinmez."
        }
        "Unstar" => "Yıldızı kaldır",
        "Star (never auto-deleted)" => "Yıldız ver (asla otomatik silinmez)",
        "Star" => "Yıldız ver",
        "Copy the clip (paste into Discord, WhatsApp…)" => "Klibi kopyala (Discord'a, WhatsApp'a… yapıştır)",
        "More" => "Daha fazla",
        "Copy for Discord (under 10 MB)" => "Discord için kopyala (10 MB altı)",
        "Making a smaller copy…" => "Küçük bir kopya hazırlanıyor…",
        "Clip copied. Paste it into Discord, WhatsApp or a folder." => "Klip kopyalandı. Discord'a, WhatsApp'a veya bir klasöre yapıştır.",
        "Couldn't copy: {e}" => "Kopyalanamadı: {e}",
        "Needs ffmpeg: install Clip previews from Add-ons." => "ffmpeg gerekiyor: Eklentiler'den Klip önizlemelerini kur.",
        "Small copy ready (under 10 MB). Paste it into Discord." => "Küçük kopya hazır (10 MB altı). Discord'a yapıştır.",
        "Couldn't make a small copy: {e}" => "Küçük kopya yapılamadı: {e}",
        "Folder per game" => "Her oyun için klasör",
        "Saves each clip into a folder named after the game you were playing." => {
            "Her klibi oynadığın oyunun adını taşıyan bir klasöre kaydeder."
        }
        "Storage limit" => "Depolama sınırı",
        "When the clips folder is bigger than this, the oldest clips are deleted for good. Starred clips are never deleted." => {
            "Klip klasörü bundan büyük olunca en eski klipler kalıcı olarak silinir. Yıldızlı klipler asla silinmez."
        }
        "Removed {n} old clips to stay under {gb} GB" => "{gb} GB altında kalmak için {n} eski klip silindi",
        "Whisper check" => "Whisper kontrolü",
        "Optional. Double-checks anything that sounds like the phrase with Whisper: far fewer mistakes, especially in Turkish. Uses more CPU for a moment after you speak. {size}" => {
            "İsteğe bağlı. Cümleye benzeyen her şeyi Whisper ile tekrar kontrol eder: özellikle Türkçede çok daha az hata. Konuştuktan sonra kısa bir süre daha fazla işlemci kullanır. {size}"
        }
        "Strictness" => "Seçicilik",
        "Relaxed" => "Rahat",
        "Normal" => "Normal",
        "Strict" => "Sıkı",
        "Relaxed misses less and allows a word or two around the phrase; Strict saves fewer clips by mistake." => {
            "Rahat daha az kaçırır ve cümlenin çevresinde bir iki kelimeye izin verir; Sıkı yanlışlıkla daha az klip kaydeder."
        }
        "Previewing {name}" => "Önizleniyor: {name}",
        "Not installed. It disappears when you leave this page." => {
            "Kurulu değil. Bu sayfadan çıkınca kaybolur."
        }
        "End preview" => "Önizlemeyi bitir",
        "Install it" => "Kur",
        "Previewing" => "Önizleniyor",
        "Loading…" => "Yükleniyor…",
        "Sound" => "Ses",
        "Rewind chime" => "Rewind zil sesi",
        "Add your own .mp3 or .wav by dragging it onto Rewind (see Add-ons)." => {
            "Kendi .mp3 veya .wav dosyanı Rewind'ın üzerine sürükleyerek ekle (Eklentiler'e bak)."
        }
        "CUSTOM SOUNDS" => "ÖZEL SESLER",
        "Clip-saved sounds" => "Klip kaydedilince çalan sesler",
        "{n} of {max} · drag an .mp3 or .wav onto Rewind to add one" => {
            "{n}/{max} · eklemek için Rewind'ın üzerine bir .mp3 veya .wav sürükle"
        }
        "{n} sounds · drag an .mp3 or .wav onto Rewind to add one" => {
            "{n} ses · eklemek için Rewind'ın üzerine bir .mp3 veya .wav sürükle"
        }
        "Open sounds folder" => "Ses klasörünü aç",
        "Plays when a clip is saved" => "Klip kaydedilince çalar",
        "Limit to 5 sounds" => "En fazla 5 ses",
        "Turn off to keep as many as you like." => "İstediğin kadar ses tutmak için kapat.",
        "Added sound: {name}" => "Ses eklendi: {name}",
        "Couldn't add the sound: {e}" => "Ses eklenemedi: {e}",
        "Test" => "Test et",
        "Stop test" => "Testi bitir",
        "Say \"{phrase}\" to test it… (closes in {s} s)" => {
            "Test etmek için \"{phrase}\" de… ({s} sn içinde kapanır)"
        }
        "Test closes in {s} s" => "Test {s} sn içinde kapanır",
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
        // Values are often messages themselves (an error from the engine, a status), so
        // they are translated too when a translation exists.
        out = out.replace(&format!("{{{k}}}"), t(v));
    }
    out
}

/// Messages built from a template plus details (an OS error, a device name). `t` can't
/// match those exactly, so each known template is recognised here and its parts are put
/// into the translation.
const TEMPLATES: &[&str] = &[
    "Desktop audio unavailable: {e}",
    "Microphone unavailable: {e}",
    "Game capture unavailable ({e}); recording the monitor only.",
    "{h} couldn't be registered ({e}). Another app may be using it.",
    "Couldn't save replay — {e}",
    "Couldn't delete: {e}",
    "Couldn't copy: {e}",
    "Couldn't update: {e}",
    "Couldn't check: {e}",
    "Couldn't install {key}: {e}",
    "Couldn't add the sound: {e}",
    "Couldn't make a small copy: {e}",
    "Couldn't apply settings: {e}",
    "Trim failed: {e}",
];

/// Translate a whole message: an exact match first, then the known templates.
pub fn tp(s: &str) -> String {
    let exact = t(s);
    if lang() == Lang::English || exact != s {
        return exact.to_string();
    }
    for template in TEMPLATES {
        if let Some(vars) = match_template(template, s) {
            let pairs: Vec<(&str, &str)> = vars.iter().map(|(k, v)| (*k, v.as_str())).collect();
            return tf(template, &pairs);
        }
    }
    s.to_string()
}

/// If `s` fits `template` ("text {name} more text"), the values of its placeholders.
fn match_template<'a>(template: &'a str, s: &str) -> Option<Vec<(&'a str, String)>> {
    // Split into literal pieces and placeholder names.
    let mut literals: Vec<&str> = Vec::new();
    let mut names: Vec<&str> = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let close = rest[open..].find('}')? + open;
        literals.push(&rest[..open]);
        names.push(&rest[open + 1..close]);
        rest = &rest[close + 1..];
    }
    literals.push(rest);
    if names.is_empty() {
        return None;
    }
    let mut pos = s.strip_prefix(literals[0])?;
    let mut out = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let next = literals[i + 1];
        let end = if i + 1 == names.len() {
            if !pos.ends_with(next) {
                return None;
            }
            pos.len() - next.len()
        } else if next.is_empty() {
            return None;
        } else {
            pos.find(next)?
        };
        out.push((*name, pos[..end].to_string()));
        pos = &pos[end + next.len()..];
    }
    Some(out)
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

#[cfg(test)]
mod template_tests {
    use super::*;

    #[test]
    fn templates_translate_their_parts() {
        set_lang(Lang::Turkish);
        assert_eq!(
            tp("Microphone unavailable: no microphone found"),
            "Mikrofon kullanılamıyor: mikrofon bulunamadı"
        );
        assert_eq!(
            tp("Couldn't copy: the clipboard is busy, try again"),
            "Kopyalanamadı: pano meşgul, tekrar dene"
        );
        assert_eq!(tp("something unknown"), "something unknown");
        set_lang(Lang::English);
        assert_eq!(tp("Microphone unavailable: x"), "Microphone unavailable: x");
    }
}
