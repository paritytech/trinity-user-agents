package io.paritytech.polkadotapp.feature_chats_impl.domain.username

import androidx.compose.ui.text.capitalize
import io.paritytech.polkadotapp.common.domain.model.AccountId
import io.paritytech.polkadotapp.common.utils.capitalize
import io.paritytech.polkadotapp.feature_chats_api.domain.username.FallbackUsernameGenerator
import java.util.Locale
import javax.inject.Inject

class RealFallbackUsernameGenerator @Inject constructor() : FallbackUsernameGenerator {
    override fun generateFromAccountId(accountId: AccountId): String {
        val bytes = accountId.value
        // Use first 2 bytes as indices into word lists
        val adjectiveIndex = (bytes[0].toInt() and 0xFF) % ADJECTIVES.size
        val nounIndex = (bytes[1].toInt() and 0xFF) % NOUNS.size

        return "${ADJECTIVES[adjectiveIndex].capitalize(Locale.US)} ${NOUNS[nounIndex].capitalize(Locale.US)}"
    }

    companion object {
        private val ADJECTIVES = listOf(
            "brave", "clever", "gentle", "swift", "bright",
            "calm", "eager", "fierce", "happy", "jolly",
            "keen", "lively", "merry", "noble", "proud",
            "quick", "royal", "sharp", "steady", "witty",
            "bold", "daring", "earnest", "fair", "grand",
            "honest", "joyful", "kind", "loyal", "mighty",
            "nimble", "polite", "quiet", "rare", "sincere",
            "tender", "unique", "vivid", "warm", "wise",
            "active", "bright", "careful", "decent", "elegant",
            "fancy", "graceful", "helpful", "ideal", "jovial",
            "knowing", "loving", "modest", "neat", "open",
            "patient", "radiant", "serene", "trusting", "upbeat",
            "valiant", "willing", "zealous", "agile", "benign",
            "cheerful", "diligent", "excited", "friendly", "genuine",
            "humble", "innocent", "jaunty", "likable", "mindful",
            "natural", "optimistic", "peaceful", "reliable", "spirited",
            "thoughtful", "useful", "vibrant", "wonderful", "youthful",
            "alert", "balanced", "capable", "devoted", "energetic",
            "focused", "grateful", "heroic", "inspired", "jubilant",
            "lasting", "magnetic", "notable", "orderly", "playful",
            "refined", "skilled", "talented", "upright", "versatile",
            "warmhearted", "adventurous", "brilliant", "composed", "dynamic",
            "fabulous", "glowing", "harmonious", "inventive", "joyous",
            "kindhearted", "luminous", "marvelous", "observant", "positive",
            "resilient", "stunning", "terrific", "unwavering", "victorious",
            "whimsical", "admirable", "blissful", "confident", "delightful",
            "enthusiastic", "fearless", "generous", "heartfelt", "imaginative",
            "jubilee", "legendary", "majestic", "nurturing", "outstanding",
            "passionate", "remarkable", "spectacular", "tremendous", "unmatched",
            "vivacious", "wonderful", "amazing", "beaming", "courageous",
            "dazzling", "exquisite", "fantastic", "gleaming", "hopeful",
            "incredible", "joyful", "kinetic", "lush", "magnificent",
            "nifty", "original", "precious", "quirky", "radiant",
            "shining", "thriving", "uplifting", "valorous", "welcoming",
            "astonishing", "breezy", "cosmic", "dreamy", "enchanting",
            "flourishing", "glorious", "heavenly", "illustrious", "jazzy",
            "kindred", "lustrous", "mystical", "nimble", "opulent",
            "pristine", "quaint", "resplendent", "sublime", "tranquil",
            "utopian", "verdant", "wondrous", "xenial", "youthful",
            "zesty", "affable", "buoyant", "captivating", "dapper",
            "effervescent", "frisky", "gallant", "hale", "intrepid",
            "jovial", "keen", "lithe", "mirthful", "natty",
            "ornate", "plucky", "quixotic", "robust", "stalwart",
            "tenacious", "urbane", "venerable", "winsome", "zany",
            "amiable", "blithe", "cordial", "debonair", "ebullient",
            "felicitous", "gregarious", "hearty", "indomitable", "jocular",
            "knightly", "laudable", "munificent", "nonchalant", "peerless",
            "quintessential", "resolute", "sanguine", "trenchant", "undaunted"
        )

        private val NOUNS = listOf(
            "beaver", "falcon", "tiger", "dolphin", "eagle",
            "panda", "wolf", "hawk", "lion", "bear",
            "otter", "raven", "shark", "whale", "zebra",
            "badger", "crane", "dragon", "elk", "fox",
            "gazelle", "heron", "ibex", "jaguar", "koala",
            "leopard", "moose", "newt", "osprey", "parrot",
            "quail", "rabbit", "salmon", "turtle", "unicorn",
            "viper", "wombat", "yak", "antelope", "buffalo",
            "cheetah", "dove", "elephant", "flamingo", "giraffe",
            "hippo", "iguana", "jellyfish", "kangaroo", "lemur",
            "meerkat", "narwhal", "owl", "penguin", "quetzal",
            "rhino", "stork", "toucan", "urchin", "vulture",
            "walrus", "xerus", "yellowjacket", "zebu", "alpaca",
            "bobcat", "cobra", "dingo", "egret", "ferret",
            "gecko", "hedgehog", "impala", "jackal", "kiwi",
            "lynx", "mongoose", "nightjar", "ocelot", "pelican",
            "quokka", "raccoon", "seahorse", "tapir", "umbrellabird",
            "vicuna", "weasel", "axolotl", "bison", "capybara",
            "dugong", "echidna", "fossa", "gibbon", "harrier",
            "ibis", "jerboa", "kookaburra", "loris", "mandrill",
            "numbat", "okapi", "puffin", "quoll", "roadrunner",
            "serval", "tamarin", "uakari", "vervet", "wallaby",
            "xenops", "yellowhammer", "zorilla", "aardvark", "banteng",
            "chinchilla", "dormouse", "ermine", "fennec", "grebe",
            "hoopoe", "indri", "jackrabbit", "kakapo", "liger",
            "manatee", "nightingale", "oriole", "platypus", "quahog",
            "robin", "starling", "tarsier", "umbrette", "vole",
            "warthog", "xantus", "yapok", "zonkey", "albatross",
            "binturong", "coypu", "dhole", "eland", "frigatebird",
            "gerenuk", "honeybadger", "isopod", "javelina", "kingfisher",
            "lemming", "markhor", "nilgai", "oncilla", "pangolin",
            "quelea", "ringtail", "sifaka", "tenrec", "urutu",
            "viscacha", "wolverine", "xeme", "yellowlegs", "zebrafish",
            "anhinga", "bushbaby", "civet", "drongo", "emu",
            "fulmar", "gannet", "hamster", "jacana", "kinkajou",
            "lapwing", "marmot", "nutria", "opossum", "porcupine",
            "quetzalcoatlus", "ratel", "skink", "treeshrew", "unau",
            "vinegaroon", "waxwing", "xenopus", "yellowtail", "zokor",
            "agouti", "bandicoot", "cassowary", "desman", "erethizon",
            "flounder", "galago", "hyrax", "iiwi", "jaguarundi",
            "kakarikis", "langur", "margay", "nighthawk", "olm",
            "potoo", "quelea", "racer", "solenodon", "thornbill",
            "urial", "verdin", "whimbrel", "xenarthra", "yellowfish",
            "zapus", "alouatta", "babirusa", "caracal", "dugite",
            "eider", "fisher", "gerbil", "hoatzin", "isabelline",
            "jabiru", "kagu", "limpkin", "muntjac", "nene",
            "oilbird", "pitta", "queleacapensis", "riflebird", "sunbittern",
            "thornbird", "umbrellacockatoo", "veery", "woodswallow", "xenicus",
            "yellowbellied", "zealandia", "amazilia", "broadbill", "coucal"
        )
    }
}
