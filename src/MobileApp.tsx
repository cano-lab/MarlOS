/**
 * Mobile Entry Point for MarlOS
 *
 * This file serves as the entry point when building for iOS/Android.
 * It uses the mobile-optimized components instead of the desktop layout.
 *
 * To use for mobile builds, update main.tsx to import this instead of App.tsx
 */

import { Component } from "solid-js";
import MobileApp from "./components/mobile/MobileApp";
import "./App.css";

const MobileEntry: Component = () => {
  return <MobileApp />;
};

export default MobileEntry;
