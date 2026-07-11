import { createApp } from "vue";
import naive from "naive-ui";
import App from "./App.vue";
import "./styles/main.css";
import { applyTheme, getThemeStorage, getThemeTokens, readTheme, resolveTheme } from "./theme";

const initialTheme = readTheme(getThemeStorage());
const initialOsTheme = window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
applyTheme(document.documentElement, resolveTheme(initialTheme, initialOsTheme), getThemeTokens(initialTheme, initialOsTheme));

const app = createApp(App);
app.use(naive);
app.mount("#app");
