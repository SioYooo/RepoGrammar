from django.db import models


class Author(models.Model):
    name = models.CharField(max_length=100)
    email = models.EmailField()

    class Meta:
        ordering = ["name"]


class Book(models.Model):
    title = models.CharField(max_length=200)
    published = models.DateField()


class Review(models.Model):
    rating = models.IntegerField()
    summary = models.TextField()
    body = models.TextField()
    score = models.IntegerField()
    approved = models.BooleanField()
