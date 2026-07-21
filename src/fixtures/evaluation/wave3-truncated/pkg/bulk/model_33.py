from pydantic import BaseModel


class Item33(BaseModel):
    id: int
    label: str
    quantity: int
