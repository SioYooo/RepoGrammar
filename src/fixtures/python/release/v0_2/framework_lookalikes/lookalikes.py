class Author(models.Model):
    name = models.CharField(max_length=100)


class Book(models.Model):
    title = models.CharField(max_length=200)


class Review(models.Model):
    rating = models.IntegerField()


app = Flask(__name__)


@app.route("/health")
def health():
    return {"ok": True}


@app.route("/users")
def users():
    return []


@app.route("/teams")
def teams():
    return []
