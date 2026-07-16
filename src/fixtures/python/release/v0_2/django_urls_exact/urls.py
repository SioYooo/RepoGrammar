from django.urls import path

from . import views

urlpatterns = [
    path("health/", views.health),
    path("users/", views.users),
    path("teams/", views.teams),
]
