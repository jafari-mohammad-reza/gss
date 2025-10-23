package p2p

import (
	"context"
	"fmt"
	"log"
	"sync"
	"time"

	"github.com/jafari-mohammad-reza/gss/pkg/config"
	"github.com/panjf2000/gnet/v2"
	"github.com/panjf2000/gnet/v2/pkg/logging"
)

type Server struct {
	conf             *config.Config
	inflightRequests sync.Map
	gnet.BuiltinEventEngine
	eng       gnet.Engine
	addr      string
	multicore bool
}

func NewServer(conf *config.Config) (*Server, error) {
	return &Server{
		conf:             conf,
		inflightRequests: sync.Map{},
	}, nil
}

func (s *Server) Start(ctx context.Context) error {
	addr := fmt.Sprintf("tcp://%s:%d", s.conf.ServerHost, s.conf.ServerPort)
	s.addr = addr
	opts := []gnet.Option{
		gnet.WithMulticore(s.multicore),
		gnet.WithLockOSThread(true),
		gnet.WithLogLevel(logging.InfoLevel),
		gnet.WithLogger(logging.GetDefaultLogger()),
	}
	errCh := make(chan error, 1)
	go func() {
		err := gnet.Run(s, addr, opts...)
		errCh <- err
	}()
	log.Printf("gnet server started on %s", addr)
	select {
	case <-ctx.Done():
		log.Printf("main context done, shutting down gnet server")
		return s.eng.Stop(context.Background())
	case err := <-errCh:
		return err
	}
}

func (s *Server) Stop(ctx context.Context) error {
	s.inflightRequests.Range(func(key, value interface{}) bool {
		wg := value.(*sync.WaitGroup)
		wg.Wait()
		return true
	})
	log.Printf("all inflight requests finished")
	return nil
}

func (s *Server) OnBoot(eng gnet.Engine) (action gnet.Action) {
	s.eng = eng
	log.Printf("gnet server booted")
	return
}

func (s *Server) OnOpened(c gnet.Conn) (out []byte, action gnet.Action) {
	reqId := fmt.Sprintf("%s:%d", c.RemoteAddr().String(), time.Now().UnixNano())
	wg := sync.WaitGroup{}
	wg.Add(1)
	s.inflightRequests.Store(reqId, &wg)
	c.SetContext(reqId)
	log.Printf("new connection from %s", c.RemoteAddr().String())
	return
}

func (s *Server) React(frame []byte, c gnet.Conn) (out []byte, action gnet.Action) {
	reqStr := string(frame)
	log.Printf("received data from %s: %s", c.RemoteAddr().String(), reqStr)
	return
}

func (s *Server) OnClosed(c gnet.Conn, err error) (action gnet.Action) {
	reqId, ok := c.Context().(string)
	if ok {
		if v, ok := s.inflightRequests.Load(reqId); ok {
			wg := v.(*sync.WaitGroup)
			wg.Done()
			s.inflightRequests.Delete(reqId)
		}
	}
	log.Printf("connection closed: %s", c.RemoteAddr().String())
	defer c.Close()
	return
}
