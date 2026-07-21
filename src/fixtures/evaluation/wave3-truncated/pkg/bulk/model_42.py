from pydantic import BaseModel


class Item42(BaseModel):
    id: int
    label: str
    quantity: int
