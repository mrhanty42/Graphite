package dev.graphite.host;

import net.neoforged.fml.loading.FMLPaths;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.FileNotFoundException;
import java.io.IOException;
import java.io.InputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

public final class NativeLoader {
    private static final Logger LOG = LoggerFactory.getLogger(NativeLoader.class);
    private static final String LIB_NAME = "graphite_core";

    private NativeLoader() {
    }

    public static void load() throws IOException {
        String devPath = System.getProperty("graphite.natives.dev_path");
        if (devPath != null) {
            Path lib = findLibInDir(Path.of(devPath));
            if (lib != null) {
                LOG.info("[Graphite] DEV MODE: loading native library from {}", lib);
                System.load(lib.toAbsolutePath().toString());
                return;
            }
        }

        String resourcePath = "/natives/" + platformDir() + "/" + libFileName();
        Path nativesDir = FMLPaths.GAMEDIR.get().resolve("graphite").resolve("natives");
        Files.createDirectories(nativesDir);
        Path target = nativesDir.resolve(libFileName());

        try (InputStream in = NativeLoader.class.getResourceAsStream(resourcePath)) {
            if (in == null) {
                throw new FileNotFoundException("Native library not found in JAR: " + resourcePath);
            }
            Files.copy(in, target, StandardCopyOption.REPLACE_EXISTING);
        }

        System.load(target.toAbsolutePath().toString());
        LOG.info("[Graphite] Native library loaded from {}", target);
    }

    private static String platformDir() {
        String os = System.getProperty("os.name").toLowerCase();
        if (os.contains("win")) {
            return "windows";
        }
        if (os.contains("mac")) {
            return "macos";
        }
        return "linux";
    }

    private static String libFileName() {
        String os = System.getProperty("os.name").toLowerCase();
        if (os.contains("win")) {
            return LIB_NAME + ".dll";
        }
        if (os.contains("mac")) {
            return "lib" + LIB_NAME + ".dylib";
        }
        return "lib" + LIB_NAME + ".so";
    }

    private static Path findLibInDir(Path dir) {
        Path candidate = dir.resolve(libFileName());
        return Files.exists(candidate) ? candidate : null;
    }
}
