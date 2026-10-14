/**
 * The asset bytes. Vite inlines each file as a `data:` URL, so the Worker
 * bundle carries them and sends them as `ChatResource::Inline`.
 */
import avatar from "../assets/avatar.png?inline";
import avatarAlt from "../assets/avatar-alt.png?inline";
import badge from "../assets/badge.png?inline";
import icon1 from "../assets/icon-1.png?inline";
import icon2 from "../assets/icon-2.png?inline";
import icon3 from "../assets/icon-3.png?inline";
import icon4 from "../assets/icon-4.png?inline";
import icon5 from "../assets/icon-5.png?inline";
import icon6 from "../assets/icon-6.png?inline";
import samplePdf from "../assets/sample.pdf?inline";
import sampleMp4 from "../assets/sample.mp4?inline";
import samplePng from "../assets/sample.png?inline";
import { dataUrlToHex } from "./bytes";
import type { Assets } from "./fixtures";

export function loadAssets(): Assets {
  return {
    avatar: dataUrlToHex(avatar),
    avatarAlt: dataUrlToHex(avatarAlt),
    badge: dataUrlToHex(badge),
    icons: [icon1, icon2, icon3, icon4, icon5, icon6].map(dataUrlToHex),
    samplePng: dataUrlToHex(samplePng),
    samplePdf: dataUrlToHex(samplePdf),
    sampleMp4: dataUrlToHex(sampleMp4),
  };
}
