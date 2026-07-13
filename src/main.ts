import { createApp } from "vue";
import App from "./App.vue";
import "./styles/main.css";
import { applyTheme, getThemeStorage, getThemeTokens, readTheme, resolveTheme } from "./theme";

const initialTheme = readTheme(getThemeStorage());
const initialOsTheme = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
applyTheme(document.documentElement, resolveTheme(initialTheme, initialOsTheme), getThemeTokens(initialTheme, initialOsTheme));

createApp(App).mount("#app");
