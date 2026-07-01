package com.example.catalog;

import org.springframework.http.ResponseEntity;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
public class CatalogController {
    @GetMapping("/catalog")
    public ResponseEntity<String> listCatalog() {
        return ResponseEntity.ok("catalog");
    }

    @GetMapping("/catalog/featured")
    public ResponseEntity<String> listFeaturedCatalog() {
        return ResponseEntity.ok("featured");
    }

    @GetMapping("/catalog/health")
    public ResponseEntity<String> catalogHealth() {
        return ResponseEntity.ok("ok");
    }
}
