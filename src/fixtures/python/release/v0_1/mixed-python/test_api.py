def test_health_response(client):
    response = client.get("/health")
    assert response.status_code == 200
