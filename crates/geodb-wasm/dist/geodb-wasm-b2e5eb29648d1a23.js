let wasm;

let cachedUint8ArrayMemory0 = null;

function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });

cachedTextDecoder.decode();

const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
}

let WASM_VECTOR_LEN = 0;

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    }
}

function passStringToWasm0(arg, malloc, realloc) {

    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }

    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let cachedDataViewMemory0 = null;

function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

export function start() {
    wasm.start();
}

/**
 * @returns {number}
 */
export function get_country_count() {
    const ret = wasm.get_country_count();
    return ret >>> 0;
}

/**
 * @param {string} iso2
 * @returns {string | undefined}
 */
export function get_country_name(iso2) {
    const ptr0 = passStringToWasm0(iso2, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.get_country_name(ptr0, len0);
    let v2;
    if (ret[0] !== 0) {
        v2 = getStringFromWasm0(ret[0], ret[1]).slice();
        wasm.__wbindgen_free(ret[0], ret[1] * 1, 1);
    }
    return v2;
}

/**
 * Searches for countries by their international phone code.
 *
 * This function takes a string that can be a full phone number or just the
 * country code prefix (e.g., "+1", "49"). It strips leading `+` characters
 * and whitespace before performing the search.
 *
 * # Arguments
 *
 * * `phone` - A string containing the phone code to search for. It can
 *   optionally be prefixed with `+`.
 *
 * # Returns
 *
 * A `JsValue` representing a JSON array of `CountryView` objects that match
 * the phone code. If multiple countries share the same code (e.g., USA and
 * Canada with "+1"), all will be returned. Returns `JsValue::NULL` if the
 * database is not initialized.
 * @param {string} phone
 * @returns {any}
 */
export function search_countries_by_phone(phone) {
    const ptr0 = passStringToWasm0(phone, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.search_countries_by_phone(ptr0, len0);
    return ret;
}

/**
 * Searches for states by a substring of their name.
 *
 * This function performs a case-insensitive search for states where the provided
 * substring appears anywhere in the state's name.
 *
 * # Arguments
 *
 * * `substr` - The substring to search for within state names (e.g., "bava" for Bavaria).
 *
 * # Returns
 *
 * A `JsValue` representing a JSON array of `StateView` objects that match the
 * search query. Each object includes details about the state and its parent
 * country. Returns `JsValue::NULL` if the database is not initialized.
 * @param {string} substr
 * @returns {any}
 */
export function search_state_substring(substr) {
    const ptr0 = passStringToWasm0(substr, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.search_state_substring(ptr0, len0);
    return ret;
}

/**
 * Searches for cities by a substring of their name.
 *
 * This function performs a case-insensitive search for cities where the provided
 * substring appears anywhere in the city's name.
 *
 * # Arguments
 *
 * * `substr` - The substring to search for within city names (e.g., "berlin").
 *
 * # Returns
 *
 * A `JsValue` representing a JSON array of `CityView` objects that match the
 * search query. Each object includes details about the city, its state, and
 * its country. Returns `JsValue::NULL` if the database is not initialized.
 * @param {string} substr
 * @returns {any}
 */
export function search_city_substring(substr) {
    const ptr0 = passStringToWasm0(substr, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.search_city_substring(ptr0, len0);
    return ret;
}

/**
 * Performs a versatile search across countries, states, and cities.
 *
 * This function intelligently interprets the query string to search for:
 * - Country names, ISO codes, or phone codes.
 * - State names or substrings.
 * - City names or substrings.
 *
 * It returns a heterogeneous array of matching items, which can include
 * countries, states, and cities.
 *
 * # Arguments
 *
 * * `query` - The search string. This can be a country ISO code (e.g., "US"),
 *   a phone code (e.g., "+1"), or a substring of a country, state, or city
 *   name (e.g., "berlin").
 *
 * # Returns
 *
 * A `JsValue` representing a JSON array of search results. The array may
 * contain a mix of `CountryView`, `StateView`, and `CityView` objects.
 * Returns an empty array if the database is not initialized or if no
 * results are found.
 * @param {string} query
 * @returns {any}
 */
export function smart_search(query) {
    const ptr0 = passStringToWasm0(query, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.smart_search(ptr0, len0);
    return ret;
}

/**
 * Retrieves statistics about the loaded database.
 *
 * This function returns a summary of the data contained within the embedded
 * GeoDB instance, such as the total number of countries, states, and cities.
 *
 * # Returns
 *
 * A `JsValue` representing a JSON object with database statistics, or
 * `JsValue::NULL` if the database has not been initialized. The object
 * structure is similar to:
 *
 * ```json
 * {
 *   "countries": 250,
 *   "states": 4993,
 *   "cities": 154933
 * }
 * ```
 * @returns {any}
 */
export function get_stats() {
    const ret = wasm.get_stats();
    return ret;
}

/**
 * Retrieves metadata about the compiled WASM module and the embedded database.
 *
 * This function provides insights into how the module was built, including the
 * specific database file that was embedded, its size, and the compile-time
 * features that were enabled. This is useful for debugging and understanding
 * which version of the data model is active.
 *
 * # Returns
 *
 * A `JsValue` representing a JSON object with build details. On failure (e.g.,
 * serialization error), it returns `JsValue::NULL`. The object has the
 * following structure:
 *
 * ```json
 * {
 *   "filename": "geodb.nested.bin",
 *   "size_bytes": 123456,
 *   "architecture": "nested",
 *   "features": {
 *     "legacy_model": true,
 *     "compact": false,
 *     "search_blobs": false
 *   }
 * }
 * ```
 * @returns {any}
 */
export function get_build_info() {
    const ret = wasm.get_build_info();
    return ret;
}

/**
 * Finds the nearest cities to a given geographic coordinate.
 *
 * This function performs a k-nearest neighbor (k-NN) search to find a specified
 * number of cities closest to the provided latitude and longitude.
 *
 * # Arguments
 *
 * * `lat` - The latitude of the search center, in decimal degrees.
 * * `lng` - The longitude of the search center, in decimal degrees.
 * * `count` - The maximum number of nearest cities to return.
 *
 * # Returns
 *
 * A `JsValue` containing a JSON array of `CityView` objects, sorted by
 * distance from the search point (closest first). Returns `JsValue::NULL` if the
 * database is not initialized.
 * @param {number} lat
 * @param {number} lng
 * @param {number} count
 * @returns {any}
 */
export function find_nearest_cities(lat, lng, count) {
    const ret = wasm.find_nearest_cities(lat, lng, count);
    return ret;
}

/**
 * Finds all cities within a specified radius from a given geographic coordinate.
 *
 * This function performs a spatial search to identify cities located within a circular
 * area defined by a center point (latitude and longitude) and a radius in kilometers.
 *
 * # Arguments
 *
 * * `lat` - The latitude of the search center, in decimal degrees.
 * * `lng` - The longitude of the search center, in decimal degrees.
 * * `radius_km` - The search radius in kilometers.
 *
 * # Returns
 *
 * A `JsValue` containing a JSON array of `CityView` objects, where each object
 * represents a city found within the radius. Returns `JsValue::NULL` if the
 * database is not initialized.
 * @param {number} lat
 * @param {number} lng
 * @param {number} radius_km
 * @returns {any}
 */
export function find_cities_in_radius(lat, lng, radius_km) {
    const ret = wasm.find_cities_in_radius(lat, lng, radius_km);
    return ret;
}

const EXPECTED_RESPONSE_TYPES = new Set(['basic', 'cors', 'default']);

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);

            } catch (e) {
                const validResponse = module.ok && EXPECTED_RESPONSE_TYPES.has(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else {
                    throw e;
                }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);

    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };

        } else {
            return instance;
        }
    }
}

function __wbg_get_imports() {
    const imports = {};
    imports.wbg = {};
    imports.wbg.__wbg_Error_e83987f665cf5504 = function(arg0, arg1) {
        const ret = Error(getStringFromWasm0(arg0, arg1));
        return ret;
    };
    imports.wbg.__wbg___wbindgen_throw_b855445ff6a94295 = function(arg0, arg1) {
        throw new Error(getStringFromWasm0(arg0, arg1));
    };
    imports.wbg.__wbg_error_7534b8e9a36f1ab4 = function(arg0, arg1) {
        let deferred0_0;
        let deferred0_1;
        try {
            deferred0_0 = arg0;
            deferred0_1 = arg1;
            console.error(getStringFromWasm0(arg0, arg1));
        } finally {
            wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
        }
    };
    imports.wbg.__wbg_log_8cec76766b8c0e33 = function(arg0) {
        console.log(arg0);
    };
    imports.wbg.__wbg_new_1acc0b6eea89d040 = function() {
        const ret = new Object();
        return ret;
    };
    imports.wbg.__wbg_new_8a6f238a6ece86ea = function() {
        const ret = new Error();
        return ret;
    };
    imports.wbg.__wbg_new_e17d9f43105b08be = function() {
        const ret = new Array();
        return ret;
    };
    imports.wbg.__wbg_push_df81a39d04db858c = function(arg0, arg1) {
        const ret = arg0.push(arg1);
        return ret;
    };
    imports.wbg.__wbg_set_3f1d0b984ed272ed = function(arg0, arg1, arg2) {
        arg0[arg1] = arg2;
    };
    imports.wbg.__wbg_set_c213c871859d6500 = function(arg0, arg1, arg2) {
        arg0[arg1 >>> 0] = arg2;
    };
    imports.wbg.__wbg_stack_0ed75d68575b0f3c = function(arg0, arg1) {
        const ret = arg1.stack;
        const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
        const len1 = WASM_VECTOR_LEN;
        getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
        getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
    };
    imports.wbg.__wbindgen_cast_2241b6af4c4b2941 = function(arg0, arg1) {
        // Cast intrinsic for `Ref(String) -> Externref`.
        const ret = getStringFromWasm0(arg0, arg1);
        return ret;
    };
    imports.wbg.__wbindgen_cast_4625c577ab2ec9ee = function(arg0) {
        // Cast intrinsic for `U64 -> Externref`.
        const ret = BigInt.asUintN(64, arg0);
        return ret;
    };
    imports.wbg.__wbindgen_cast_d6cd19b81560fd6e = function(arg0) {
        // Cast intrinsic for `F64 -> Externref`.
        const ret = arg0;
        return ret;
    };
    imports.wbg.__wbindgen_init_externref_table = function() {
        const table = wasm.__wbindgen_externrefs;
        const offset = table.grow(4);
        table.set(0, undefined);
        table.set(offset + 0, undefined);
        table.set(offset + 1, null);
        table.set(offset + 2, true);
        table.set(offset + 3, false);
        ;
    };

    return imports;
}

function __wbg_finalize_init(instance, module) {
    wasm = instance.exports;
    __wbg_init.__wbindgen_wasm_module = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;


    wasm.__wbindgen_start();
    return wasm;
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (typeof module !== 'undefined') {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();

    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }

    const instance = new WebAssembly.Instance(module, imports);

    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (typeof module_or_path !== 'undefined') {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (typeof module_or_path === 'undefined') {
        module_or_path = new URL('geodb-wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync };
export default __wbg_init;
