from sqlalchemy import select
from sqlalchemy.orm import Session

from models import User


class UserRepository:
    def list_users(self, session: Session):
        return session.execute(select(User)).scalars().all()

    def create_user(self, session: Session, email: str):
        user = User(email=email)
        session.add(user)
        session.commit()
        return user
