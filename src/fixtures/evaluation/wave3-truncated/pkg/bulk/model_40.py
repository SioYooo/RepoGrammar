from pydantic import BaseModel


class Item40(BaseModel):
    id: int
    label: str
    quantity: int
