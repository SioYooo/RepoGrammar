package com.example.catalog;

import jakarta.persistence.Entity;
import jakarta.persistence.Id;
import jakarta.persistence.OneToMany;

@Entity
class Book {
    @Id
    Long id;

    @OneToMany
    java.util.List<Page> pages;
}

@Entity
class Author {
    @Id
    Long id;
}

@Entity
class Publisher {
    @Id
    Long id;
}
