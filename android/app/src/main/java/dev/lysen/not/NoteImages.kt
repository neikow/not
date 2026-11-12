package dev.lysen.not

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.ImageDecoder
import android.net.Uri
import android.util.LruCache
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.remember
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.platform.LocalContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.ByteArrayOutputStream
import kotlin.math.max
import kotlin.math.roundToInt

/*
 * Images in notes: picked from the photo picker, downscaled and re-encoded here, then kept by the
 * core under a content id (ADR 0010). Notes show them decoded at screen width, cached in memory.
 */

/** Longest side images are kept at: sharp on a phone or laptop screen, a few hundred KB as JPEG. */
private const val MAX_SIDE = 2048
private const val JPEG_QUALITY = 85

/** Decodes the picked image (EXIF rotation applied), downscales it and encodes JPEG, or PNG when it has transparency. */
suspend fun encodeForNote(context: Context, uri: Uri): ByteArray = withContext(Dispatchers.IO) {
    val source = ImageDecoder.createSource(context.contentResolver, uri)
    val bitmap = ImageDecoder.decodeBitmap(source) { decoder, info, _ ->
        val side = max(info.size.width, info.size.height)
        if (side > MAX_SIDE) {
            val scale = MAX_SIDE.toFloat() / side
            decoder.setTargetSize((info.size.width * scale).roundToInt(), (info.size.height * scale).roundToInt())
        }
        // Software, so it can be compressed.
        decoder.allocator = ImageDecoder.ALLOCATOR_SOFTWARE
    }
    ByteArrayOutputStream().use { out ->
        if (bitmap.hasAlpha()) bitmap.compress(Bitmap.CompressFormat.PNG, 100, out)
        else bitmap.compress(Bitmap.CompressFormat.JPEG, JPEG_QUALITY, out)
        out.toByteArray()
    }
}

/** Decoded images by id, shared by every editor; about 48 MB of pixels at most. */
private object DecodedImages : LruCache<String, ImageBitmap>(48 * 1024 * 1024) {
    override fun sizeOf(key: String, value: ImageBitmap) = value.width * value.height * 4
}

/**
 * The images [ids] name, decoded to at most [maxWidth] px wide, as they load; an id is missing
 * while it decodes, or until sync brings it ([reloadKey] changes then, e.g. the store's version).
 */
@Composable
fun rememberNoteImages(ids: List<String>, maxWidth: Int, reloadKey: Any?): Map<String, ImageBitmap> {
    val store = (LocalContext.current.applicationContext as NotApp).store
    val loaded = remember { mutableStateMapOf<String, ImageBitmap>() }
    LaunchedEffect(ids, maxWidth, reloadKey) {
        if (maxWidth <= 0) return@LaunchedEffect
        for (id in ids) {
            if (id in loaded) continue
            val bitmap = DecodedImages.get(id) ?: withContext(Dispatchers.IO) {
                val bytes = runCatching { store.image(id) }.getOrNull() ?: return@withContext null
                decodeToWidth(bytes, maxWidth)?.asImageBitmap()?.also { DecodedImages.put(id, it) }
            }
            if (bitmap != null) loaded[id] = bitmap
        }
    }
    return loaded
}

private fun decodeToWidth(bytes: ByteArray, maxWidth: Int): Bitmap? {
    val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
    BitmapFactory.decodeByteArray(bytes, 0, bytes.size, bounds)
    if (bounds.outWidth <= 0) return null
    var sample = 1
    while (bounds.outWidth / (sample * 2) >= maxWidth) sample *= 2
    return BitmapFactory.decodeByteArray(bytes, 0, bytes.size, BitmapFactory.Options().apply { inSampleSize = sample })
}
