! Bounded positive fixture: top-level external test subroutines with their own
! use statements, plus a driver program whose internal procedure never anchors.
program tester
  use, intrinsic :: iso_fortran_env, only : error_unit
  use testdrive, only : run_testsuite, new_testsuite, testsuite_type
  use test_drive_top, only : collect_top
  implicit none
  integer :: stat
  type(testsuite_type), allocatable :: testsuites(:)

  stat = 0
  testsuites = [ new_testsuite("top", collect_top) ]
  call run_testsuite(testsuites(1)%collect, error_unit, stat)
  if (stat > 0) error stop 1

contains

  subroutine report_failure()
    ! An internal procedure of the program: never a test-drive entry.
  end subroutine report_failure

end program tester

subroutine test_alpha(error)
  use testdrive
  implicit none
  type(error_type), allocatable, intent(out) :: error
end subroutine test_alpha

subroutine test_beta(error)
  use testdrive
  implicit none
  type(error_type), allocatable, intent(out) :: error
end subroutine test_beta

subroutine test_gamma(error)
  use testdrive
  implicit none
  type(error_type), allocatable, intent(out) :: error
end subroutine test_gamma
