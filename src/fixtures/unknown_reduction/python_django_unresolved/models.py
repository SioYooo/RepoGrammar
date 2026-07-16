from myapp import models


class Post(models.Model):
    title = models.CharField(max_length=200)


class Tag(models.Model):
    label = models.CharField(max_length=50)


class Comment(models.Model):
    body = models.TextField()
