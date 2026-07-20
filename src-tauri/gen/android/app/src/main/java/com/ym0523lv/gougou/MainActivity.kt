package com.ym0523lv.gougou

import android.os.Bundle
import android.webkit.WebView
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat

class MainActivity : TauriActivity() {
  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
  }

  override fun onWebViewCreate(webView: WebView) {
    ViewCompat.setOnApplyWindowInsetsListener(webView) { view, insets ->
      val imeBottom = insets.getInsets(WindowInsetsCompat.Type.ime()).bottom
      val imeCssPixels = imeBottom / view.resources.displayMetrics.density
      webView.evaluateJavascript(
        "document.documentElement.style.setProperty('--keyboard-inset-height', '${imeCssPixels}px')",
        null,
      )
      insets
    }
    ViewCompat.requestApplyInsets(webView)
  }
}
